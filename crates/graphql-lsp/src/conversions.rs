//! Type conversion functions between LSP types and graphql-ide types
//!
//! This module contains pure conversion functions that translate between:
//! - LSP protocol types (from tower-lsp/lsp-types)
//! - graphql-ide types (our internal IDE API types)
//!
//! These conversions are stateless and can be used from any LSP handler.

use lsp_types::{Diagnostic, DiagnosticSeverity, Location, Position, Range};

/// Convert LSP Position to graphql-ide Position
pub const fn convert_lsp_position(pos: Position) -> graphql_ide::Position {
    graphql_ide::Position::new(pos.line, pos.character)
}

/// Convert graphql-ide Position to LSP Position
pub const fn convert_ide_position(pos: graphql_ide::Position) -> Position {
    Position {
        line: pos.line,
        character: pos.character,
    }
}

/// Convert graphql-ide Range to LSP Range
pub const fn convert_ide_range(range: graphql_ide::Range) -> Range {
    Range {
        start: convert_ide_position(range.start),
        end: convert_ide_position(range.end),
    }
}

/// Convert graphql-ide Location to LSP Location
pub fn convert_ide_location(loc: &graphql_ide::Location) -> Location {
    Location {
        uri: loc.file.as_str().parse().expect("Invalid URI"),
        range: convert_ide_range(loc.range),
    }
}

/// Convert graphql-ide `CompletionItem` to LSP `CompletionItem`
pub fn convert_ide_completion_item(item: graphql_ide::CompletionItem) -> lsp_types::CompletionItem {
    lsp_types::CompletionItem {
        label: item.label,
        kind: Some(match item.kind {
            graphql_ide::CompletionKind::Field => lsp_types::CompletionItemKind::FIELD,
            graphql_ide::CompletionKind::Type => lsp_types::CompletionItemKind::CLASS,
            graphql_ide::CompletionKind::Fragment => lsp_types::CompletionItemKind::SNIPPET,
            graphql_ide::CompletionKind::Directive => lsp_types::CompletionItemKind::KEYWORD,
            graphql_ide::CompletionKind::EnumValue => lsp_types::CompletionItemKind::ENUM_MEMBER,
            graphql_ide::CompletionKind::Argument => lsp_types::CompletionItemKind::PROPERTY,
            graphql_ide::CompletionKind::Variable => lsp_types::CompletionItemKind::VARIABLE,
        }),
        detail: item.detail,
        documentation: item.documentation.map(|doc| {
            lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: doc,
            })
        }),
        deprecated: Some(item.deprecated),
        insert_text: item.insert_text,
        ..Default::default()
    }
}

/// Convert graphql-ide `HoverResult` to LSP Hover
pub fn convert_ide_hover(hover: graphql_ide::HoverResult) -> lsp_types::Hover {
    lsp_types::Hover {
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: hover.contents,
        }),
        range: hover.range.map(convert_ide_range),
    }
}

/// Convert graphql-ide Diagnostic to LSP Diagnostic
pub fn convert_ide_diagnostic(diag: graphql_ide::Diagnostic) -> Diagnostic {
    let severity = match diag.severity {
        graphql_ide::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        graphql_ide::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        graphql_ide::DiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        graphql_ide::DiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    };

    Diagnostic {
        range: convert_ide_range(diag.range),
        severity: Some(severity),
        code: diag.code.map(lsp_types::NumberOrString::String),
        source: Some(diag.source),
        message: diag.message,
        ..Default::default()
    }
}

/// Convert graphql-ide `SymbolKind` to LSP `SymbolKind`
pub const fn convert_ide_symbol_kind(kind: graphql_ide::SymbolKind) -> lsp_types::SymbolKind {
    match kind {
        graphql_ide::SymbolKind::Type | graphql_ide::SymbolKind::Fragment => {
            lsp_types::SymbolKind::CLASS
        }
        graphql_ide::SymbolKind::Field => lsp_types::SymbolKind::FIELD,
        graphql_ide::SymbolKind::Query
        | graphql_ide::SymbolKind::Mutation
        | graphql_ide::SymbolKind::Subscription => lsp_types::SymbolKind::FUNCTION,
        graphql_ide::SymbolKind::EnumValue => lsp_types::SymbolKind::ENUM_MEMBER,
        graphql_ide::SymbolKind::Scalar => lsp_types::SymbolKind::TYPE_PARAMETER,
        graphql_ide::SymbolKind::Input => lsp_types::SymbolKind::STRUCT,
        graphql_ide::SymbolKind::Interface => lsp_types::SymbolKind::INTERFACE,
        graphql_ide::SymbolKind::Union | graphql_ide::SymbolKind::Enum => {
            lsp_types::SymbolKind::ENUM
        }
    }
}

/// Convert graphql-ide `DocumentSymbol` to LSP `DocumentSymbol`
#[allow(deprecated)] // LSP requires deprecated field
pub fn convert_ide_document_symbol(
    symbol: graphql_ide::DocumentSymbol,
) -> lsp_types::DocumentSymbol {
    lsp_types::DocumentSymbol {
        name: symbol.name,
        kind: convert_ide_symbol_kind(symbol.kind),
        detail: symbol.detail,
        range: convert_ide_range(symbol.range),
        selection_range: convert_ide_range(symbol.selection_range),
        children: if symbol.children.is_empty() {
            None
        } else {
            Some(
                symbol
                    .children
                    .into_iter()
                    .map(convert_ide_document_symbol)
                    .collect(),
            )
        },
        tags: None,
        deprecated: None,
    }
}

/// Convert graphql-ide `WorkspaceSymbol` to LSP `WorkspaceSymbol`
#[allow(deprecated)] // LSP requires deprecated field
pub fn convert_ide_workspace_symbol(
    symbol: graphql_ide::WorkspaceSymbol,
) -> lsp_types::WorkspaceSymbol {
    lsp_types::WorkspaceSymbol {
        name: symbol.name,
        kind: convert_ide_symbol_kind(symbol.kind),
        location: lsp_types::OneOf::Left(convert_ide_location(&symbol.location)),
        container_name: symbol.container_name,
        tags: None,
        data: None,
    }
}
