#![allow(clippy::needless_pass_by_value)]

use crate::conversions::{
    convert_ide_document_symbol, convert_ide_location, convert_ide_workspace_symbol,
    convert_lsp_position,
};
use crate::global_state::{GlobalState, GlobalStateSnapshot};
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Location, ReferenceParams,
};

pub(crate) fn handle_goto_definition(
    snap: GlobalStateSnapshot,
    params: GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let position = convert_lsp_position(params.text_document_position_params.position);
    let locations = snap.analysis.goto_definition(&snap.file_path, position)?;
    let lsp_locations: Vec<Location> = locations.iter().map(convert_ide_location).collect();
    if lsp_locations.is_empty() {
        None
    } else {
        Some(GotoDefinitionResponse::Array(lsp_locations))
    }
}

pub(crate) fn handle_references(
    snap: GlobalStateSnapshot,
    params: ReferenceParams,
) -> Option<Vec<Location>> {
    let position = convert_lsp_position(params.text_document_position.position);
    let include_declaration = params.context.include_declaration;
    let locations =
        snap.analysis
            .find_references(&snap.file_path, position, include_declaration)?;
    let lsp_locations: Vec<Location> = locations
        .into_iter()
        .map(|loc| convert_ide_location(&loc))
        .collect();
    if lsp_locations.is_empty() {
        None
    } else {
        Some(lsp_locations)
    }
}

pub(crate) fn handle_document_symbol(
    snap: GlobalStateSnapshot,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let _ = params;
    let symbols = snap.analysis.document_symbols(&snap.file_path);
    if symbols.is_empty() {
        return None;
    }
    let lsp_symbols: Vec<lsp_types::DocumentSymbol> = symbols
        .into_iter()
        .map(convert_ide_document_symbol)
        .collect();
    Some(DocumentSymbolResponse::Nested(lsp_symbols))
}

pub(crate) fn handle_workspace_symbol(
    state: &mut GlobalState,
    params: lsp_types::WorkspaceSymbolParams,
) -> Option<lsp_types::WorkspaceSymbolResponse> {
    let mut all_symbols = Vec::new();

    for (_, host) in state.workspace.all_hosts() {
        let analysis = host.snapshot();
        let symbols = analysis.workspace_symbols(&params.query);
        for symbol in symbols {
            all_symbols.push(convert_ide_workspace_symbol(symbol));
        }
    }

    if all_symbols.is_empty() {
        return None;
    }
    Some(lsp_types::WorkspaceSymbolResponse::Nested(all_symbols))
}
