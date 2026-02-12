//! Inlay hints feature implementation.
//!
//! This module provides IDE inlay hints functionality:
//! - Field return types (displayed after scalar field selections)
//!
//! Note: Variable definition types are NOT shown as hints since they already
//! have explicit type annotations in the GraphQL syntax.

use std::collections::HashMap;
use std::sync::Arc;

use apollo_parser::cst::{CstNode, Definition, Selection};

use crate::helpers::{format_type_ref, offset_to_position};
use crate::types::{FilePath, InlayHint, InlayHintKind, Position, Range};
use crate::FileRegistry;

/// Get inlay hints for a file.
///
/// Returns inlay hints showing return types after scalar field selections.
/// Hints are only shown for leaf fields (fields without nested selection sets)
/// since the return type is not visible in the query syntax.
///
/// If `range` is provided, only returns hints within that range for efficiency.
pub fn inlay_hints(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
    range: Option<Range>,
) -> Vec<InlayHint> {
    let (content, metadata) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata)
    };

    let Some(project_files) = project_files else {
        return Vec::new();
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let schema_types = graphql_hir::schema_types(db, project_files);

    let mut hints = Vec::new();

    for doc in parse.documents() {
        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
        let line_offset = doc.line_offset;

        collect_hints_from_tree(
            doc.tree,
            schema_types,
            &doc_line_index,
            line_offset,
            range,
            &mut hints,
        );
    }

    hints
}

/// Collect inlay hints from a syntax tree
fn collect_hints_from_tree(
    tree: &apollo_parser::SyntaxTree,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    range: Option<Range>,
    hints: &mut Vec<InlayHint>,
) {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                let root_type = match op.operation_type() {
                    Some(op_type) if op_type.mutation_token().is_some() => "Mutation",
                    Some(op_type) if op_type.subscription_token().is_some() => "Subscription",
                    _ => "Query",
                };

                // Collect field hints from selection set
                if let Some(selection_set) = op.selection_set() {
                    collect_selection_set_hints(
                        &selection_set,
                        root_type,
                        schema_types,
                        line_index,
                        line_offset,
                        range,
                        hints,
                    );
                }
            }
            Definition::FragmentDefinition(frag) => {
                let fragment_type = frag
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|n| n.text().to_string());

                if let (Some(type_name), Some(selection_set)) =
                    (fragment_type, frag.selection_set())
                {
                    collect_selection_set_hints(
                        &selection_set,
                        &type_name,
                        schema_types,
                        line_index,
                        line_offset,
                        range,
                        hints,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Collect field type hints from a selection set
fn collect_selection_set_hints(
    selection_set: &apollo_parser::cst::SelectionSet,
    parent_type: &str,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    range: Option<Range>,
    hints: &mut Vec<InlayHint>,
) {
    // Early return if parent type is unknown - no type info available for hints
    let Some(type_def) = schema_types.get(parent_type) else {
        return;
    };

    for selection in selection_set.selections() {
        match selection {
            Selection::Field(field) => {
                if let Some(name) = field.name() {
                    let field_name = name.text();

                    // Get position after the alias (if present) or field name.
                    // We use the alias position because that's what appears in the result.
                    let end_node = field.alias().map_or_else(
                        || name.syntax().text_range().end(),
                        |alias| alias.syntax().text_range().end(),
                    );

                    // Handle __typename introspection field specially
                    // It's always available on any type and returns String!
                    if field_name == "__typename" {
                        if field.selection_set().is_none() {
                            let end_offset: usize = end_node.into();
                            let position = offset_to_position(line_index, end_offset);
                            let adjusted = adjust_position_for_line_offset(position, line_offset);

                            if should_include_position(adjusted, range) {
                                hints.push(InlayHint::new(
                                    adjusted,
                                    ": String!".to_string(),
                                    InlayHintKind::Type,
                                ));
                            }
                        }
                        continue;
                    }

                    // Find field type in schema
                    if let Some(field_def) = type_def
                        .fields
                        .iter()
                        .find(|f| f.name.as_ref() == field_name)
                    {
                        // If there's no selection set, show the type hint
                        // (for scalar fields, showing the type is most useful)
                        if field.selection_set().is_none() {
                            let end_offset: usize = end_node.into();
                            let position = offset_to_position(line_index, end_offset);
                            let adjusted = adjust_position_for_line_offset(position, line_offset);

                            if should_include_position(adjusted, range) {
                                let type_str = format_type_ref(&field_def.type_ref);
                                hints.push(InlayHint::new(
                                    adjusted,
                                    format!(": {type_str}"),
                                    InlayHintKind::Type,
                                ));
                            }
                        }

                        // Recurse into nested selection sets
                        if let Some(nested) = field.selection_set() {
                            let field_type_name = field_def.type_ref.name.as_ref();
                            collect_selection_set_hints(
                                &nested,
                                field_type_name,
                                schema_types,
                                line_index,
                                line_offset,
                                range,
                                hints,
                            );
                        }
                    }
                }
            }
            Selection::InlineFragment(inline_frag) => {
                let fragment_type = inline_frag
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map_or_else(|| parent_type.to_string(), |n| n.text().to_string());

                if let Some(nested) = inline_frag.selection_set() {
                    collect_selection_set_hints(
                        &nested,
                        &fragment_type,
                        schema_types,
                        line_index,
                        line_offset,
                        range,
                        hints,
                    );
                }
            }
            Selection::FragmentSpread(_) => {
                // Fragment spreads don't get type hints here - the fragment definition has them
            }
        }
    }
}

/// Adjust position for line offset (for embedded GraphQL in TS/JS)
const fn adjust_position_for_line_offset(position: Position, line_offset: u32) -> Position {
    if line_offset == 0 {
        position
    } else {
        Position::new(position.line + line_offset, position.character)
    }
}

/// Check if a position should be included based on the requested range.
/// Uses proper 2D range comparison (line and character).
fn should_include_position(position: Position, range: Option<Range>) -> bool {
    let Some(range) = range else {
        return true;
    };

    // Check if position is within the 2D range
    if position.line < range.start.line || position.line > range.end.line {
        return false;
    }

    // Handle same-line ranges
    if position.line == range.start.line && position.character < range.start.character {
        return false;
    }

    if position.line == range.end.line && position.character > range.end.character {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_include_position_no_range() {
        let pos = Position::new(5, 10);
        assert!(should_include_position(pos, None));
    }

    #[test]
    fn test_should_include_position_in_range() {
        let pos = Position::new(5, 10);
        let range = Range::new(Position::new(0, 0), Position::new(10, 0));
        assert!(should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_out_of_range() {
        let pos = Position::new(15, 10);
        let range = Range::new(Position::new(0, 0), Position::new(10, 0));
        assert!(!should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_same_line_before_start() {
        // Position is on the start line but before the start character
        let pos = Position::new(5, 5);
        let range = Range::new(Position::new(5, 10), Position::new(5, 20));
        assert!(!should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_same_line_after_end() {
        // Position is on the end line but after the end character
        let pos = Position::new(5, 50);
        let range = Range::new(Position::new(5, 10), Position::new(5, 20));
        assert!(!should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_same_line_in_range() {
        // Position is within a same-line range
        let pos = Position::new(5, 15);
        let range = Range::new(Position::new(5, 10), Position::new(5, 20));
        assert!(should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_at_range_start() {
        // Position exactly at range start should be included
        let pos = Position::new(5, 10);
        let range = Range::new(Position::new(5, 10), Position::new(5, 20));
        assert!(should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_at_range_end() {
        // Position exactly at range end should be included
        let pos = Position::new(5, 20);
        let range = Range::new(Position::new(5, 10), Position::new(5, 20));
        assert!(should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_at_multiline_start() {
        // Position exactly at range start on multi-line range
        let pos = Position::new(3, 5);
        let range = Range::new(Position::new(3, 5), Position::new(10, 15));
        assert!(should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_should_include_position_at_multiline_end() {
        // Position exactly at range end on multi-line range
        let pos = Position::new(10, 15);
        let range = Range::new(Position::new(3, 5), Position::new(10, 15));
        assert!(should_include_position(pos, Some(range)));
    }

    #[test]
    fn test_adjust_position_no_offset() {
        let pos = Position::new(5, 10);
        let adjusted = adjust_position_for_line_offset(pos, 0);
        assert_eq!(adjusted.line, 5);
        assert_eq!(adjusted.character, 10);
    }

    #[test]
    fn test_adjust_position_with_offset() {
        let pos = Position::new(5, 10);
        let adjusted = adjust_position_for_line_offset(pos, 3);
        assert_eq!(adjusted.line, 8);
        assert_eq!(adjusted.character, 10);
    }
}
