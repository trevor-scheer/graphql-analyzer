// Handlers take ownership of snapshot + params because they run on worker threads.
#![allow(clippy::needless_pass_by_value)]

use crate::conversions::{
    convert_ide_code_lens, convert_ide_code_lens_info, convert_ide_folding_range,
    convert_ide_hover, convert_ide_inlay_hint, convert_ide_location, convert_ide_selection_range,
    convert_lsp_position,
};
use crate::global_state::GlobalStateSnapshot;
use lsp_types::{
    CodeLens, CodeLensParams, FoldingRange, FoldingRangeParams, Hover, HoverParams,
    InlayHint as LspInlayHint, InlayHintParams, SelectionRange, SelectionRangeParams,
    SemanticToken, SemanticTokens, SemanticTokensParams, SemanticTokensResult, Uri,
};
use std::str::FromStr;

pub(crate) fn handle_hover(snap: GlobalStateSnapshot, params: HoverParams) -> Option<Hover> {
    let position = convert_lsp_position(params.text_document_position_params.position);
    snap.analysis
        .hover(&snap.file_path, position)
        .map(convert_ide_hover)
}

pub(crate) fn handle_semantic_tokens_full(
    snap: GlobalStateSnapshot,
    params: SemanticTokensParams,
) -> Option<SemanticTokensResult> {
    let _ = params;
    let tokens = snap.analysis.semantic_tokens(&snap.file_path);
    if tokens.is_empty() {
        return None;
    }

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

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: encoded_tokens,
    }))
}

pub(crate) fn handle_selection_range(
    snap: GlobalStateSnapshot,
    params: SelectionRangeParams,
) -> Option<Vec<SelectionRange>> {
    let positions: Vec<graphql_ide::Position> = params
        .positions
        .iter()
        .map(|p| convert_lsp_position(*p))
        .collect();

    let selection_ranges = snap.analysis.selection_ranges(&snap.file_path, &positions);
    let lsp_ranges: Vec<SelectionRange> = selection_ranges
        .into_iter()
        .filter_map(|sr| sr.map(convert_ide_selection_range))
        .collect();
    if lsp_ranges.is_empty() {
        None
    } else {
        Some(lsp_ranges)
    }
}

pub(crate) fn handle_code_lens(
    snap: GlobalStateSnapshot,
    params: CodeLensParams,
) -> Option<Vec<CodeLens>> {
    let _ = params;
    let uri = match Uri::from_str(&snap.file_path.0) {
        Ok(uri) => uri,
        Err(e) => {
            tracing::warn!(
                path = %snap.file_path.0,
                error = %e,
                "code_lens: failed to parse FilePath as URI, skipping",
            );
            return None;
        }
    };

    let mut lsp_code_lenses: Vec<CodeLens> = Vec::new();

    let deprecated_lenses = snap.analysis.deprecated_field_code_lenses(&snap.file_path);
    lsp_code_lenses.extend(
        deprecated_lenses
            .iter()
            .map(|cl| convert_ide_code_lens_info(cl, &uri)),
    );

    let fragment_lenses = snap.analysis.code_lenses(&snap.file_path);
    for lens in &fragment_lenses {
        let fragment_name = lens
            .command
            .as_ref()
            .and_then(|cmd| cmd.arguments.get(2))
            .map(String::as_str);

        let references: Vec<lsp_types::Location> = if let Some(name) = fragment_name {
            snap.analysis
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
        return None;
    }

    Some(lsp_code_lenses)
}

pub(crate) fn handle_folding_range(
    snap: GlobalStateSnapshot,
    params: FoldingRangeParams,
) -> Option<Vec<FoldingRange>> {
    let _ = params;
    let ranges = snap.analysis.folding_ranges(&snap.file_path);
    if ranges.is_empty() {
        return None;
    }
    let lsp_ranges: Vec<FoldingRange> = ranges.iter().map(convert_ide_folding_range).collect();
    Some(lsp_ranges)
}

pub(crate) fn handle_inlay_hint(
    snap: GlobalStateSnapshot,
    params: InlayHintParams,
) -> Option<Vec<LspInlayHint>> {
    let range = Some(graphql_ide::Range::new(
        graphql_ide::Position::new(params.range.start.line, params.range.start.character),
        graphql_ide::Position::new(params.range.end.line, params.range.end.character),
    ));

    let hints = snap.analysis.inlay_hints(&snap.file_path, range);
    if hints.is_empty() {
        return None;
    }
    let lsp_hints: Vec<LspInlayHint> = hints.iter().map(convert_ide_inlay_hint).collect();
    Some(lsp_hints)
}
