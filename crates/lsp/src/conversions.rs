//! Type conversion functions between LSP types and graphql-ide types
//!
//! This module contains pure conversion functions that translate between:
//! - LSP protocol types (from tower-lsp/lsp-types)
//! - graphql-ide types (our internal IDE API types)
//!
//! These conversions are stateless and can be used from any LSP handler.
//!
//! ## Extension Traits
//!
//! For ergonomic conversions, use the extension traits:
//!
//! ```rust,ignore
//! use crate::conversions::{IntoLsp, IntoIde};
//!
//! // IDE -> LSP
//! let lsp_position = ide_position.into_lsp();
//! let lsp_range = ide_range.into_lsp();
//!
//! // LSP -> IDE
//! let ide_position = lsp_position.into_ide();
//! ```

use lsp_types::{
    CodeLens, Command, Diagnostic, DiagnosticSeverity, FoldingRange, FoldingRangeKind, InlayHint,
    InlayHintKind, InlayHintLabel, Location, Position, Range, Uri,
};

// =============================================================================
// Extension Traits
// =============================================================================

/// Extension trait for converting graphql-ide types to LSP types.
///
/// Provides ergonomic method-style conversion: `ide_type.into_lsp()`
#[allow(dead_code)] // Public API - implementations below are used internally
pub trait IntoLsp {
    /// The LSP type this converts to
    type Output;
    /// Convert to the corresponding LSP type
    fn into_lsp(self) -> Self::Output;
}

/// Extension trait for converting LSP types to graphql-ide types.
///
/// Provides ergonomic method-style conversion: `lsp_type.into_ide()`
#[allow(dead_code)] // Public API for future use
pub trait IntoIde {
    /// The IDE type this converts to
    type Output;
    /// Convert to the corresponding IDE type
    fn into_ide(self) -> Self::Output;
}

// Position conversions
impl IntoLsp for graphql_ide::Position {
    type Output = Position;
    fn into_lsp(self) -> Position {
        convert_ide_position(self)
    }
}

impl IntoIde for Position {
    type Output = graphql_ide::Position;
    fn into_ide(self) -> graphql_ide::Position {
        convert_lsp_position(self)
    }
}

// Range conversions
impl IntoLsp for graphql_ide::Range {
    type Output = Range;
    fn into_lsp(self) -> Range {
        convert_ide_range(self)
    }
}

// Location conversions (reference version for efficiency)
impl IntoLsp for &graphql_ide::Location {
    type Output = Location;
    fn into_lsp(self) -> Location {
        convert_ide_location(self)
    }
}

// Diagnostic conversions
impl IntoLsp for graphql_ide::Diagnostic {
    type Output = Diagnostic;
    fn into_lsp(self) -> Diagnostic {
        convert_ide_diagnostic(self)
    }
}

// HoverResult conversions
impl IntoLsp for graphql_ide::HoverResult {
    type Output = lsp_types::Hover;
    fn into_lsp(self) -> lsp_types::Hover {
        convert_ide_hover(self)
    }
}

// CompletionItem conversions
impl IntoLsp for graphql_ide::CompletionItem {
    type Output = lsp_types::CompletionItem;
    fn into_lsp(self) -> lsp_types::CompletionItem {
        convert_ide_completion_item(self)
    }
}

// DocumentSymbol conversions
impl IntoLsp for graphql_ide::DocumentSymbol {
    type Output = lsp_types::DocumentSymbol;
    fn into_lsp(self) -> lsp_types::DocumentSymbol {
        convert_ide_document_symbol(self)
    }
}

// WorkspaceSymbol conversions
impl IntoLsp for graphql_ide::WorkspaceSymbol {
    type Output = lsp_types::WorkspaceSymbol;
    fn into_lsp(self) -> lsp_types::WorkspaceSymbol {
        convert_ide_workspace_symbol(self)
    }
}

// FoldingRange conversions (reference version)
impl IntoLsp for &graphql_ide::FoldingRange {
    type Output = FoldingRange;
    fn into_lsp(self) -> FoldingRange {
        convert_ide_folding_range(self)
    }
}

// InlayHint conversions (reference version)
impl IntoLsp for &graphql_ide::InlayHint {
    type Output = InlayHint;
    fn into_lsp(self) -> InlayHint {
        convert_ide_inlay_hint(self)
    }
}

// =============================================================================
// Standalone Conversion Functions (for backward compatibility)
// =============================================================================

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
        insert_text_format: item.insert_text_format.map(|format| match format {
            graphql_ide::InsertTextFormat::PlainText => lsp_types::InsertTextFormat::PLAIN_TEXT,
            graphql_ide::InsertTextFormat::Snippet => lsp_types::InsertTextFormat::SNIPPET,
        }),
        sort_text: item.sort_text,
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

/// Convert graphql-ide `CodeLensInfo` to LSP `CodeLens`
///
/// Creates a code lens that shows the usage count for deprecated fields.
/// When clicked, it navigates to the usages using the "find all references" command.
pub fn convert_ide_code_lens_info(info: &graphql_ide::CodeLensInfo, uri: &Uri) -> CodeLens {
    let title = if info.usage_count == 1 {
        "1 usage remaining".to_string()
    } else {
        format!("{} usages remaining", info.usage_count)
    };

    // Create the command that will be executed when the code lens is clicked.
    // We use our custom graphql-analyzer.showReferences command which handles the
    // JSON-to-VSCode type conversion. See editors/vscode/src/extension.ts for
    // why this wrapper is necessary (LSP sends JSON, but VSCode commands need
    // native types with methods).
    let command = if info.usage_count > 0 {
        // Convert IDE locations to LSP locations for the command arguments
        let lsp_locations: Vec<Location> = info
            .usage_locations
            .iter()
            .map(convert_ide_location)
            .collect();

        Some(Command {
            title,
            command: "graphql-analyzer.showReferences".to_string(),
            arguments: Some(vec![
                serde_json::to_value(uri.to_string()).unwrap(),
                serde_json::to_value(convert_ide_position(info.range.start)).unwrap(),
                serde_json::to_value(lsp_locations).unwrap(),
            ]),
        })
    } else {
        // For code lenses with 0 usages, just show the title (no action)
        Some(Command {
            title,
            command: String::new(),
            arguments: None,
        })
    };

    CodeLens {
        range: convert_ide_range(info.range),
        command,
        data: None,
    }
}

/// Convert graphql-ide `CodeLens` to LSP `CodeLens`
///
/// Creates a code lens for fragment definitions showing reference counts.
/// When clicked, it triggers the find references command at the fragment location.
pub fn convert_ide_code_lens(
    lens: &graphql_ide::CodeLens,
    uri: &Uri,
    references: &[Location],
) -> CodeLens {
    let command = if references.is_empty() {
        // No references - show title only (no action)
        Some(Command {
            title: lens.title.clone(),
            command: String::new(),
            arguments: None,
        })
    } else {
        // Has references - make clickable to show them
        Some(Command {
            title: lens.title.clone(),
            command: "graphql-analyzer.showReferences".to_string(),
            arguments: Some(vec![
                serde_json::to_value(uri.to_string()).unwrap(),
                serde_json::to_value(convert_ide_position(lens.range.start)).unwrap(),
                serde_json::to_value(references).unwrap(),
            ]),
        })
    };

    CodeLens {
        range: convert_ide_range(lens.range),
        command,
        data: None,
    }
}

/// Convert graphql-ide `FoldingRange` to LSP `FoldingRange`
pub fn convert_ide_folding_range(range: &graphql_ide::FoldingRange) -> FoldingRange {
    FoldingRange {
        start_line: range.start_line,
        start_character: None,
        end_line: range.end_line,
        end_character: None,
        kind: Some(match range.kind {
            graphql_ide::FoldingRangeKind::Region => FoldingRangeKind::Region,
            graphql_ide::FoldingRangeKind::Comment => FoldingRangeKind::Comment,
        }),
        collapsed_text: None,
    }
}

/// Convert graphql-ide `InlayHint` to LSP `InlayHint`
pub fn convert_ide_inlay_hint(hint: &graphql_ide::InlayHint) -> InlayHint {
    InlayHint {
        position: convert_ide_position(hint.position),
        label: InlayHintLabel::String(hint.label.clone()),
        kind: Some(match hint.kind {
            graphql_ide::InlayHintKind::Type => InlayHintKind::TYPE,
            graphql_ide::InlayHintKind::Parameter => InlayHintKind::PARAMETER,
        }),
        text_edits: None,
        tooltip: None,
        padding_left: Some(hint.padding_left),
        padding_right: Some(hint.padding_right),
        data: None,
    }
}

/// Convert graphql-ide `SelectionRange` to LSP `SelectionRange`
///
/// Selection ranges form a linked list from innermost to outermost,
/// used by the "Expand Selection" (Shift+Alt+Right) feature.
pub fn convert_ide_selection_range(
    selection_range: graphql_ide::SelectionRange,
) -> lsp_types::SelectionRange {
    lsp_types::SelectionRange {
        range: convert_ide_range(selection_range.range),
        parent: selection_range
            .parent
            .map(|parent| Box::new(convert_ide_selection_range(*parent))),
    }
}
