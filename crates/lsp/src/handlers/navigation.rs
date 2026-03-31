use crate::conversions::{
    convert_ide_document_symbol, convert_ide_location, convert_ide_workspace_symbol,
    convert_lsp_position,
};
use crate::server::GraphQLLanguageServer;
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Location, ReferenceParams,
};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types as lsp_types;

pub(crate) async fn handle_goto_definition(
    server: &GraphQLLanguageServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let lsp_position = params.text_document_position_params.position;
    let position = convert_lsp_position(lsp_position);

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let locations = analysis.goto_definition(&file_path, position)?;
            let lsp_locations: Vec<Location> = locations.iter().map(convert_ide_location).collect();
            if lsp_locations.is_empty() {
                None
            } else {
                Some(GotoDefinitionResponse::Array(lsp_locations))
            }
        })
        .await
}

pub(crate) async fn handle_references(
    server: &GraphQLLanguageServer,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let lsp_position = params.text_document_position.position;
    let include_declaration = params.context.include_declaration;
    let position = convert_lsp_position(lsp_position);

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let locations = analysis.find_references(&file_path, position, include_declaration)?;
            let lsp_locations: Vec<Location> = locations
                .into_iter()
                .map(|loc| convert_ide_location(&loc))
                .collect();
            if lsp_locations.is_empty() {
                None
            } else {
                Some(lsp_locations)
            }
        })
        .await
}

pub(crate) async fn handle_document_symbol(
    server: &GraphQLLanguageServer,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;
    tracing::debug!("Document symbols requested: {}", uri.path());

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let symbols = analysis.document_symbols(&file_path);
            if symbols.is_empty() {
                tracing::debug!("No symbols found in document");
                return None;
            }
            let lsp_symbols: Vec<lsp_types::DocumentSymbol> = symbols
                .into_iter()
                .map(convert_ide_document_symbol)
                .collect();
            tracing::debug!("Returning {} document symbols", lsp_symbols.len());
            Some(DocumentSymbolResponse::Nested(lsp_symbols))
        })
        .await
}

pub(crate) async fn handle_workspace_symbol(
    server: &GraphQLLanguageServer,
    params: lsp_types::WorkspaceSymbolParams,
) -> Result<Option<lsp_types::WorkspaceSymbolResponse>> {
    tracing::debug!("Workspace symbols requested: {}", params.query);

    let mut all_symbols = Vec::new();

    for host in server.workspace.all_hosts() {
        let Some(analysis) = host.try_snapshot().await else {
            // Skip this host if we can't acquire the lock in time
            continue;
        };

        let symbols = analysis.workspace_symbols(&params.query);
        for symbol in symbols {
            all_symbols.push(convert_ide_workspace_symbol(symbol));
        }
    }

    if all_symbols.is_empty() {
        tracing::debug!("No workspace symbols found matching query");
        return Ok(None);
    }

    tracing::debug!("Returning {} workspace symbols", all_symbols.len());
    Ok(Some(lsp_types::WorkspaceSymbolResponse::Nested(
        all_symbols,
    )))
}
