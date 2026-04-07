use crate::conversions::{
    convert_ide_code_lens, convert_ide_code_lens_info, convert_ide_folding_range,
    convert_ide_hover, convert_ide_inlay_hint, convert_ide_location, convert_ide_selection_range,
    convert_lsp_position,
};
use crate::server::GraphQLLanguageServer;
use lsp_types::{
    CodeLens, CodeLensParams, FoldingRange, FoldingRangeParams, Hover, HoverParams,
    InlayHint as LspInlayHint, InlayHintParams, SelectionRange, SelectionRangeParams,
    SemanticToken, SemanticTokens, SemanticTokensParams, SemanticTokensResult, Uri,
};
use std::str::FromStr;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types as lsp_types;

pub(crate) async fn handle_hover(
    server: &GraphQLLanguageServer,
    params: HoverParams,
) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let lsp_position = params.text_document_position_params.position;
    let position = convert_lsp_position(lsp_position);

    server
        .with_analysis(&uri, move |analysis, file_path| {
            analysis.hover(&file_path, position).map(convert_ide_hover)
        })
        .await
}

pub(crate) async fn handle_semantic_tokens_full(
    server: &GraphQLLanguageServer,
    params: SemanticTokensParams,
) -> Result<Option<SemanticTokensResult>> {
    let uri = params.text_document.uri;
    tracing::debug!("Semantic tokens requested: {}", uri.path());

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let tokens = analysis.semantic_tokens(&file_path);
            if tokens.is_empty() {
                tracing::debug!("No semantic tokens found in document");
                return None;
            }

            // Convert to LSP delta-encoded format
            let mut encoded_tokens = Vec::with_capacity(tokens.len() * 5);
            let mut prev_line = 0u32;
            let mut prev_start = 0u32;

            for token in tokens {
                let delta_line = token.start.line - prev_line;
                let delta_start = if delta_line == 0 {
                    token.start.character - prev_start
                } else {
                    token.start.character
                };

                encoded_tokens.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length: token.length,
                    token_type: token.token_type.index(),
                    token_modifiers_bitset: token.modifiers.raw(),
                });

                prev_line = token.start.line;
                prev_start = token.start.character;
            }

            // Log any deprecated tokens for debugging
            let deprecated_count = encoded_tokens
                .iter()
                .filter(|t| t.token_modifiers_bitset != 0)
                .count();
            if deprecated_count > 0 {
                tracing::debug!(
                    "Found {} tokens with modifiers (deprecated or definition)",
                    deprecated_count
                );
                for token in encoded_tokens
                    .iter()
                    .filter(|t| t.token_modifiers_bitset != 0)
                {
                    tracing::debug!(
                        "  Token with modifiers_bitset={}",
                        token.token_modifiers_bitset
                    );
                }
            }

            tracing::debug!(count = encoded_tokens.len(), "Returning semantic tokens");
            Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: encoded_tokens,
            }))
        })
        .await
}

pub(crate) async fn handle_selection_range(
    server: &GraphQLLanguageServer,
    params: SelectionRangeParams,
) -> Result<Option<Vec<SelectionRange>>> {
    let uri = params.text_document.uri;
    tracing::debug!("Selection range requested: {}", uri.path());

    let positions: Vec<graphql_ide::Position> = params
        .positions
        .iter()
        .map(|p| convert_lsp_position(*p))
        .collect();

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let selection_ranges = analysis.selection_ranges(&file_path, &positions);
            let lsp_ranges: Vec<SelectionRange> = selection_ranges
                .into_iter()
                .filter_map(|sr| sr.map(convert_ide_selection_range))
                .collect();
            if lsp_ranges.is_empty() {
                tracing::debug!("No selection ranges found");
                None
            } else {
                tracing::debug!(count = lsp_ranges.len(), "Returning selection ranges");
                Some(lsp_ranges)
            }
        })
        .await
}

#[tracing::instrument(skip(server, params), fields(path = %params.text_document.uri.path()))]
pub(crate) async fn handle_code_lens(
    server: &GraphQLLanguageServer,
    params: CodeLensParams,
) -> Result<Option<Vec<CodeLens>>> {
    let uri = params.text_document.uri;

    server
        .with_analysis(&uri, move |analysis, file_path| {
            // If the FilePath doesn't round-trip through `Uri::from_str` (rare,
            // happens for virtual / in-memory schemes), log and skip rather
            // than panicking the spawn_blocking worker.
            let uri = match Uri::from_str(&file_path.0) {
                Ok(uri) => uri,
                Err(e) => {
                    tracing::warn!(
                        path = %file_path.0,
                        error = %e,
                        "code_lens: failed to parse FilePath as URI, skipping",
                    );
                    return None;
                }
            };
            let mut lsp_code_lenses: Vec<CodeLens> = Vec::new();

            // Code lenses for deprecated fields (in schema files)
            let deprecated_lenses = analysis.deprecated_field_code_lenses(&file_path);
            lsp_code_lenses.extend(
                deprecated_lenses
                    .iter()
                    .map(|cl| convert_ide_code_lens_info(cl, &uri)),
            );

            // Code lenses for fragment definitions (showing reference counts)
            let fragment_lenses = analysis.code_lenses(&file_path);
            for lens in &fragment_lenses {
                let fragment_name = lens
                    .command
                    .as_ref()
                    .and_then(|cmd| cmd.arguments.get(2))
                    .map(String::as_str);

                let references: Vec<lsp_types::Location> = if let Some(name) = fragment_name {
                    analysis
                        .find_fragment_references(name, false)
                        .iter()
                        .map(convert_ide_location)
                        .collect()
                } else {
                    Vec::new()
                };

                lsp_code_lenses.push(convert_ide_code_lens(lens, &uri, &references));
            }

            if lsp_code_lenses.is_empty() {
                tracing::debug!("No code lenses found");
                return None;
            }

            tracing::debug!(count = lsp_code_lenses.len(), "Returning code lenses");
            Some(lsp_code_lenses)
        })
        .await
}

#[allow(clippy::unused_async)]
pub(crate) async fn handle_code_lens_resolve(code_lens: CodeLens) -> Result<CodeLens> {
    // Code lens is already resolved with command, just return it
    Ok(code_lens)
}

#[tracing::instrument(skip(server, params), fields(path = %params.text_document.uri.path()))]
pub(crate) async fn handle_folding_range(
    server: &GraphQLLanguageServer,
    params: FoldingRangeParams,
) -> Result<Option<Vec<FoldingRange>>> {
    let uri = params.text_document.uri;

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let ranges = analysis.folding_ranges(&file_path);
            if ranges.is_empty() {
                tracing::debug!("No folding ranges found");
                return None;
            }
            let lsp_ranges: Vec<FoldingRange> =
                ranges.iter().map(convert_ide_folding_range).collect();
            tracing::debug!(count = lsp_ranges.len(), "Returning folding ranges");
            Some(lsp_ranges)
        })
        .await
}

#[tracing::instrument(skip(server, params), fields(path = %params.text_document.uri.path()))]
pub(crate) async fn handle_inlay_hint(
    server: &GraphQLLanguageServer,
    params: InlayHintParams,
) -> Result<Option<Vec<LspInlayHint>>> {
    let uri = params.text_document.uri;

    let range = Some(graphql_ide::Range::new(
        graphql_ide::Position::new(params.range.start.line, params.range.start.character),
        graphql_ide::Position::new(params.range.end.line, params.range.end.character),
    ));

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let hints = analysis.inlay_hints(&file_path, range);
            if hints.is_empty() {
                tracing::debug!("No inlay hints found");
                return None;
            }
            let lsp_hints: Vec<LspInlayHint> = hints.iter().map(convert_ide_inlay_hint).collect();
            tracing::debug!(count = lsp_hints.len(), "Returning inlay hints");
            Some(lsp_hints)
        })
        .await
}
