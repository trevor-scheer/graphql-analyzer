//! Type conversion functions between LSP types and graphql-ide types
//!
//! This module contains pure conversion functions that translate between:
//! - LSP protocol types (from tower-lsp/lsp-types)
//! - graphql-ide types (our internal IDE API types)
//!
//! These conversions are stateless and can be used from any LSP handler.

use lsp_types::{
    CodeLens, Command, Diagnostic, DiagnosticSeverity, FoldingRange, FoldingRangeKind, InlayHint,
    InlayHintKind, InlayHintLabel, Location, Position, Range, Uri,
};
use tower_lsp_server::ls_types as lsp_types;

// =============================================================================
// Conversion Functions
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
            graphql_ide::CompletionKind::Directive | graphql_ide::CompletionKind::Keyword => {
                lsp_types::CompletionItemKind::KEYWORD
            }
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

    // If a URL fails to parse, drop it rather than panic — the diagnostic
    // itself is still useful without the documentation link.
    let code_description = diag
        .url
        .as_deref()
        .and_then(|url| url.parse().ok())
        .map(|href| lsp_types::CodeDescription { href });

    let tags: Vec<lsp_types::DiagnosticTag> = diag
        .tags
        .iter()
        .map(|t| match t {
            graphql_ide::DiagnosticTag::Unnecessary => lsp_types::DiagnosticTag::UNNECESSARY,
            graphql_ide::DiagnosticTag::Deprecated => lsp_types::DiagnosticTag::DEPRECATED,
        })
        .collect();

    // LSP has no dedicated `help` field, so we append help text to the message.
    // Clients that render `codeDescription` will still see the doc link separately.
    let mut message = diag.message;
    if let Some(ref help) = diag.help {
        message = format!("{message}\nhelp: {help}");
    }

    Diagnostic {
        range: convert_ide_range(diag.range),
        severity: Some(severity),
        code: diag.code.map(lsp_types::NumberOrString::String),
        code_description,
        source: Some(diag.source),
        message,
        tags: if tags.is_empty() { None } else { Some(tags) },
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
        graphql_ide::SymbolKind::Directive => lsp_types::SymbolKind::EVENT,
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
                serde_json::to_value(uri.to_string()).expect("String is always serializable"),
                serde_json::to_value(convert_ide_position(info.range.start))
                    .expect("Position is always serializable"),
                serde_json::to_value(lsp_locations).expect("Vec<Location> is always serializable"),
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
                serde_json::to_value(uri.to_string()).expect("String is always serializable"),
                serde_json::to_value(convert_ide_position(lens.range.start))
                    .expect("Position is always serializable"),
                serde_json::to_value(references).expect("&[Location] is always serializable"),
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

/// Convert graphql-ide `SignatureHelp` to LSP `SignatureHelp`
pub fn convert_ide_signature_help(help: graphql_ide::SignatureHelp) -> lsp_types::SignatureHelp {
    lsp_types::SignatureHelp {
        signatures: help
            .signatures
            .into_iter()
            .map(|sig| lsp_types::SignatureInformation {
                label: sig.label,
                documentation: sig.documentation.map(|doc| {
                    lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: doc,
                    })
                }),
                parameters: Some(
                    sig.parameters
                        .into_iter()
                        .map(|param| lsp_types::ParameterInformation {
                            label: lsp_types::ParameterLabel::LabelOffsets([
                                param.label_offsets.0,
                                param.label_offsets.1,
                            ]),
                            documentation: param.documentation.map(|doc| {
                                lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                                    kind: lsp_types::MarkupKind::Markdown,
                                    value: doc,
                                })
                            }),
                        })
                        .collect(),
                ),
                active_parameter: None,
            })
            .collect(),
        active_signature: help.active_signature,
        active_parameter: help.active_parameter,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_lsp_position() {
        let lsp_pos = Position {
            line: 5,
            character: 10,
        };
        let ide_pos = convert_lsp_position(lsp_pos);
        assert_eq!(ide_pos.line, 5);
        assert_eq!(ide_pos.character, 10);
    }

    #[test]
    fn test_convert_ide_position() {
        let ide_pos = graphql_ide::Position::new(3, 7);
        let lsp_pos = convert_ide_position(ide_pos);
        assert_eq!(lsp_pos.line, 3);
        assert_eq!(lsp_pos.character, 7);
    }

    #[test]
    fn test_convert_ide_range() {
        let ide_range = graphql_ide::Range {
            start: graphql_ide::Position::new(1, 0),
            end: graphql_ide::Position::new(5, 10),
        };
        let lsp_range = convert_ide_range(ide_range);
        assert_eq!(lsp_range.start.line, 1);
        assert_eq!(lsp_range.start.character, 0);
        assert_eq!(lsp_range.end.line, 5);
        assert_eq!(lsp_range.end.character, 10);
    }

    #[test]
    fn test_convert_ide_location() {
        let ide_loc = graphql_ide::Location::new(
            graphql_ide::FilePath::new("file:///test.graphql"),
            graphql_ide::Range {
                start: graphql_ide::Position::new(0, 0),
                end: graphql_ide::Position::new(1, 5),
            },
        );
        let lsp_loc = convert_ide_location(&ide_loc);
        assert_eq!(lsp_loc.uri.as_str(), "file:///test.graphql");
        assert_eq!(lsp_loc.range.start.line, 0);
        assert_eq!(lsp_loc.range.end.line, 1);
    }

    #[test]
    fn test_convert_ide_completion_item_field() {
        let ide_item = graphql_ide::CompletionItem::new(
            "name".to_string(),
            graphql_ide::CompletionKind::Field,
        );
        let lsp_item = convert_ide_completion_item(ide_item);
        assert_eq!(lsp_item.label, "name");
        assert_eq!(lsp_item.kind, Some(lsp_types::CompletionItemKind::FIELD));
    }

    #[test]
    fn test_convert_ide_completion_item_fragment() {
        let ide_item = graphql_ide::CompletionItem::new(
            "UserFields".to_string(),
            graphql_ide::CompletionKind::Fragment,
        );
        let lsp_item = convert_ide_completion_item(ide_item);
        assert_eq!(lsp_item.label, "UserFields");
        assert_eq!(lsp_item.kind, Some(lsp_types::CompletionItemKind::SNIPPET));
    }

    #[test]
    fn test_convert_ide_completion_item_with_detail() {
        let ide_item =
            graphql_ide::CompletionItem::new("id".to_string(), graphql_ide::CompletionKind::Field)
                .with_detail("ID!".to_string());
        let lsp_item = convert_ide_completion_item(ide_item);
        assert_eq!(lsp_item.detail, Some("ID!".to_string()));
    }

    #[test]
    fn test_convert_ide_hover() {
        let ide_hover = graphql_ide::HoverResult {
            contents: "**User**\nA user in the system".to_string(),
            range: Some(graphql_ide::Range {
                start: graphql_ide::Position::new(0, 0),
                end: graphql_ide::Position::new(0, 4),
            }),
        };
        let lsp_hover = convert_ide_hover(ide_hover);
        if let lsp_types::HoverContents::Markup(markup) = lsp_hover.contents {
            assert!(markup.value.contains("User"));
        } else {
            panic!("Expected markup contents");
        }
    }

    #[test]
    fn test_convert_ide_diagnostic_error() {
        let ide_diag = graphql_ide::Diagnostic {
            severity: graphql_ide::DiagnosticSeverity::Error,
            message: "Unknown field".to_string(),
            range: graphql_ide::Range::new(
                graphql_ide::Position::new(1, 2),
                graphql_ide::Position::new(1, 10),
            ),
            source: "graphql".to_string(),
            code: Some("unknown-field".to_string()),
            fix: None,
            help: None,
            url: None,
            tags: Vec::new(),
        };
        let lsp_diag = convert_ide_diagnostic(ide_diag);
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp_diag.message, "Unknown field");
        assert_eq!(lsp_diag.source, Some("graphql".to_string()));
    }

    #[test]
    fn test_convert_ide_diagnostic_warning() {
        let ide_diag = graphql_ide::Diagnostic {
            severity: graphql_ide::DiagnosticSeverity::Warning,
            message: "Deprecated field".to_string(),
            range: graphql_ide::Range::new(
                graphql_ide::Position::new(0, 0),
                graphql_ide::Position::new(0, 0),
            ),
            source: "linter".to_string(),
            code: None,
            fix: None,
            help: None,
            url: None,
            tags: Vec::new(),
        };
        let lsp_diag = convert_ide_diagnostic(ide_diag);
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn test_convert_ide_diagnostic_with_help_appends_to_message() {
        let ide_diag = graphql_ide::Diagnostic {
            severity: graphql_ide::DiagnosticSeverity::Warning,
            message: "Field is deprecated".to_string(),
            range: graphql_ide::Range::new(
                graphql_ide::Position::new(0, 0),
                graphql_ide::Position::new(0, 0),
            ),
            source: "linter".to_string(),
            code: Some("noDeprecated".to_string()),
            fix: None,
            help: Some("Use the replacement field".to_string()),
            url: None,
            tags: Vec::new(),
        };
        let lsp_diag = convert_ide_diagnostic(ide_diag);
        assert_eq!(
            lsp_diag.message,
            "Field is deprecated\nhelp: Use the replacement field"
        );
    }

    #[test]
    fn test_convert_ide_diagnostic_with_url_sets_code_description() {
        let ide_diag = graphql_ide::Diagnostic {
            severity: graphql_ide::DiagnosticSeverity::Warning,
            message: "msg".to_string(),
            range: graphql_ide::Range::new(
                graphql_ide::Position::new(0, 0),
                graphql_ide::Position::new(0, 0),
            ),
            source: "linter".to_string(),
            code: Some("noDeprecated".to_string()),
            fix: None,
            help: None,
            url: Some("https://graphql-analyzer.dev/rules/noDeprecated".to_string()),
            tags: Vec::new(),
        };
        let lsp_diag = convert_ide_diagnostic(ide_diag);
        let desc = lsp_diag
            .code_description
            .expect("code_description should be set when url is provided");
        assert_eq!(
            desc.href.as_str(),
            "https://graphql-analyzer.dev/rules/noDeprecated"
        );
    }

    #[test]
    fn test_convert_ide_diagnostic_with_invalid_url_drops_code_description() {
        let ide_diag = graphql_ide::Diagnostic {
            severity: graphql_ide::DiagnosticSeverity::Warning,
            message: "msg".to_string(),
            range: graphql_ide::Range::new(
                graphql_ide::Position::new(0, 0),
                graphql_ide::Position::new(0, 0),
            ),
            source: "linter".to_string(),
            code: None,
            fix: None,
            help: None,
            url: Some("not a valid url".to_string()),
            tags: Vec::new(),
        };
        let lsp_diag = convert_ide_diagnostic(ide_diag);
        assert!(
            lsp_diag.code_description.is_none(),
            "invalid URL should be dropped rather than panic"
        );
    }

    #[test]
    fn test_convert_ide_diagnostic_tags() {
        let ide_diag = graphql_ide::Diagnostic {
            severity: graphql_ide::DiagnosticSeverity::Warning,
            message: "msg".to_string(),
            range: graphql_ide::Range::new(
                graphql_ide::Position::new(0, 0),
                graphql_ide::Position::new(0, 0),
            ),
            source: "linter".to_string(),
            code: None,
            fix: None,
            help: None,
            url: None,
            tags: vec![
                graphql_ide::DiagnosticTag::Unnecessary,
                graphql_ide::DiagnosticTag::Deprecated,
            ],
        };
        let lsp_diag = convert_ide_diagnostic(ide_diag);
        let tags = lsp_diag.tags.expect("tags should be present");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], lsp_types::DiagnosticTag::UNNECESSARY);
        assert_eq!(tags[1], lsp_types::DiagnosticTag::DEPRECATED);
    }

    #[test]
    fn test_convert_ide_symbol_kind() {
        assert_eq!(
            convert_ide_symbol_kind(graphql_ide::SymbolKind::Type),
            lsp_types::SymbolKind::CLASS
        );
        assert_eq!(
            convert_ide_symbol_kind(graphql_ide::SymbolKind::Field),
            lsp_types::SymbolKind::FIELD
        );
        assert_eq!(
            convert_ide_symbol_kind(graphql_ide::SymbolKind::Query),
            lsp_types::SymbolKind::FUNCTION
        );
        assert_eq!(
            convert_ide_symbol_kind(graphql_ide::SymbolKind::Scalar),
            lsp_types::SymbolKind::TYPE_PARAMETER
        );
        assert_eq!(
            convert_ide_symbol_kind(graphql_ide::SymbolKind::Interface),
            lsp_types::SymbolKind::INTERFACE
        );
        assert_eq!(
            convert_ide_symbol_kind(graphql_ide::SymbolKind::Directive),
            lsp_types::SymbolKind::EVENT
        );
    }

    #[test]
    fn test_convert_ide_folding_range() {
        let ide_range = graphql_ide::FoldingRange {
            start_line: 0,
            end_line: 5,
            kind: graphql_ide::FoldingRangeKind::Region,
        };
        let lsp_range = convert_ide_folding_range(&ide_range);
        assert_eq!(lsp_range.start_line, 0);
        assert_eq!(lsp_range.end_line, 5);
        assert_eq!(lsp_range.kind, Some(FoldingRangeKind::Region));
    }

    #[test]
    fn test_convert_ide_inlay_hint() {
        let ide_hint = graphql_ide::InlayHint {
            position: graphql_ide::Position::new(1, 5),
            label: ": String".to_string(),
            kind: graphql_ide::InlayHintKind::Type,
            padding_left: true,
            padding_right: false,
        };
        let lsp_hint = convert_ide_inlay_hint(&ide_hint);
        assert_eq!(lsp_hint.position.line, 1);
        assert_eq!(lsp_hint.position.character, 5);
        assert_eq!(lsp_hint.kind, Some(InlayHintKind::TYPE));
        assert_eq!(lsp_hint.padding_left, Some(true));
        assert_eq!(lsp_hint.padding_right, Some(false));
    }
}
