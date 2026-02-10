//! Folding ranges feature implementation.
//!
//! This module provides IDE folding range functionality for GraphQL documents:
//! - Selection sets `{ ... }`
//! - Operation definitions (query, mutation, subscription)
//! - Fragment definitions
//! - Multi-line block comments

use crate::helpers::{adjust_range_for_line_offset, offset_range_to_range};
use crate::types::{FilePath, FoldingRange, FoldingRangeKind};
use crate::FileRegistry;
use apollo_parser::cst::{CstNode, Definition};

/// Get folding ranges for a file.
///
/// Returns folding ranges for:
/// - Operation definitions (query, mutation, subscription)
/// - Fragment definitions
/// - Selection sets
/// - Block comments
pub fn folding_ranges(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    file: &FilePath,
) -> Vec<FoldingRange> {
    let Some(file_id) = registry.get_file_id(file) else {
        return Vec::new();
    };

    let Some(content) = registry.get_content(file_id) else {
        return Vec::new();
    };

    let Some(metadata) = registry.get_metadata(file_id) else {
        return Vec::new();
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let mut ranges = Vec::new();

    for doc in parse.documents() {
        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
        let line_offset = doc.line_offset;

        let doc_cst = doc.tree.document();

        // Collect folding ranges from definitions
        for definition in doc_cst.definitions() {
            collect_definition_folding_ranges(
                &definition,
                &doc_line_index,
                line_offset,
                &mut ranges,
            );
        }

        // Collect block comments from tokens
        collect_comment_folding_ranges(doc.tree, &doc_line_index, line_offset, &mut ranges);
    }

    // Sort by start line and deduplicate
    ranges.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(a.end_line.cmp(&b.end_line))
    });
    ranges.dedup();

    ranges
}

/// Collect folding ranges from a definition
fn collect_definition_folding_ranges(
    definition: &Definition,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<FoldingRange>,
) {
    match definition {
        Definition::OperationDefinition(op) => {
            // Fold the entire operation if it spans multiple lines
            let op_range = op.syntax().text_range();
            add_multiline_range(
                op_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );

            // Also fold selection sets within the operation
            if let Some(selection_set) = op.selection_set() {
                collect_selection_set_folding_ranges(
                    &selection_set,
                    line_index,
                    line_offset,
                    ranges,
                );
            }
        }
        Definition::FragmentDefinition(frag) => {
            // Fold the entire fragment if it spans multiple lines
            let frag_range = frag.syntax().text_range();
            add_multiline_range(
                frag_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );

            // Also fold selection sets within the fragment
            if let Some(selection_set) = frag.selection_set() {
                collect_selection_set_folding_ranges(
                    &selection_set,
                    line_index,
                    line_offset,
                    ranges,
                );
            }
        }
        Definition::ObjectTypeDefinition(obj) => {
            let obj_range = obj.syntax().text_range();
            add_multiline_range(
                obj_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );

            if let Some(fields) = obj.fields_definition() {
                let fields_range = fields.syntax().text_range();
                add_multiline_range(
                    fields_range,
                    line_index,
                    line_offset,
                    FoldingRangeKind::Region,
                    ranges,
                );
            }
        }
        Definition::InterfaceTypeDefinition(iface) => {
            let iface_range = iface.syntax().text_range();
            add_multiline_range(
                iface_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );

            if let Some(fields) = iface.fields_definition() {
                let fields_range = fields.syntax().text_range();
                add_multiline_range(
                    fields_range,
                    line_index,
                    line_offset,
                    FoldingRangeKind::Region,
                    ranges,
                );
            }
        }
        Definition::InputObjectTypeDefinition(input) => {
            let input_range = input.syntax().text_range();
            add_multiline_range(
                input_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );

            if let Some(fields) = input.input_fields_definition() {
                let fields_range = fields.syntax().text_range();
                add_multiline_range(
                    fields_range,
                    line_index,
                    line_offset,
                    FoldingRangeKind::Region,
                    ranges,
                );
            }
        }
        Definition::EnumTypeDefinition(enum_def) => {
            let enum_range = enum_def.syntax().text_range();
            add_multiline_range(
                enum_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );

            if let Some(values) = enum_def.enum_values_definition() {
                let values_range = values.syntax().text_range();
                add_multiline_range(
                    values_range,
                    line_index,
                    line_offset,
                    FoldingRangeKind::Region,
                    ranges,
                );
            }
        }
        Definition::UnionTypeDefinition(union_def) => {
            let union_range = union_def.syntax().text_range();
            add_multiline_range(
                union_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );
        }
        Definition::ScalarTypeDefinition(scalar) => {
            let scalar_range = scalar.syntax().text_range();
            add_multiline_range(
                scalar_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );
        }
        Definition::DirectiveDefinition(directive) => {
            let directive_range = directive.syntax().text_range();
            add_multiline_range(
                directive_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );
        }
        Definition::SchemaDefinition(schema) => {
            let schema_range = schema.syntax().text_range();
            add_multiline_range(
                schema_range,
                line_index,
                line_offset,
                FoldingRangeKind::Region,
                ranges,
            );
        }
        _ => {}
    }
}

/// Recursively collect folding ranges from selection sets
fn collect_selection_set_folding_ranges(
    selection_set: &apollo_parser::cst::SelectionSet,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<FoldingRange>,
) {
    let set_range = selection_set.syntax().text_range();
    add_multiline_range(
        set_range,
        line_index,
        line_offset,
        FoldingRangeKind::Region,
        ranges,
    );

    // Recursively process nested selection sets
    for selection in selection_set.selections() {
        match selection {
            apollo_parser::cst::Selection::Field(field) => {
                if let Some(nested_set) = field.selection_set() {
                    collect_selection_set_folding_ranges(
                        &nested_set,
                        line_index,
                        line_offset,
                        ranges,
                    );
                }
            }
            apollo_parser::cst::Selection::InlineFragment(inline) => {
                if let Some(nested_set) = inline.selection_set() {
                    collect_selection_set_folding_ranges(
                        &nested_set,
                        line_index,
                        line_offset,
                        ranges,
                    );
                }
            }
            apollo_parser::cst::Selection::FragmentSpread(_) => {
                // Fragment spreads don't have nested selection sets in the current document
            }
        }
    }
}

/// Collect block comment folding ranges from the syntax tree
fn collect_comment_folding_ranges(
    tree: &apollo_parser::SyntaxTree,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<FoldingRange>,
) {
    // Walk through all tokens looking for comments
    for token in tree.document().syntax().descendants_with_tokens() {
        if let apollo_parser::SyntaxElement::Token(token) = token {
            // Check if this is a comment token
            // Block comments in GraphQL are enclosed in triple quotes: """..."""
            let text = token.text();
            if text.starts_with("\"\"\"") && text.ends_with("\"\"\"") && text.len() > 6 {
                let token_range = token.text_range();
                add_multiline_range(
                    token_range,
                    line_index,
                    line_offset,
                    FoldingRangeKind::Comment,
                    ranges,
                );
            }
            // Single-line comments starting with # don't need folding
        }
    }
}

/// Add a folding range if it spans multiple lines
fn add_multiline_range(
    text_range: apollo_parser::TextRange,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    kind: FoldingRangeKind,
    ranges: &mut Vec<FoldingRange>,
) {
    let start: usize = text_range.start().into();
    let end: usize = text_range.end().into();

    let ide_range = offset_range_to_range(line_index, start, end);
    let adjusted_range = adjust_range_for_line_offset(ide_range, line_offset);

    // Only add if it spans multiple lines
    if adjusted_range.start.line < adjusted_range.end.line {
        ranges.push(FoldingRange::new(
            adjusted_range.start.line,
            adjusted_range.end.line,
            kind,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnalysisHost, DocumentKind, Language};

    #[test]
    fn test_folding_ranges_operation() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { id: ID!, name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_path,
            r"query GetUser {
  user {
    id
    name
  }
}",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let ranges = snapshot.folding_ranges(&query_path);

        // Should have folding ranges for the operation and selection sets
        assert!(!ranges.is_empty(), "Should have folding ranges");

        // The operation itself should be foldable (lines 0-5)
        let has_operation_fold = ranges.iter().any(|r| r.start_line == 0 && r.end_line == 5);
        assert!(
            has_operation_fold,
            "Should have operation folding range, got: {ranges:?}"
        );
    }

    #[test]
    fn test_folding_ranges_nested_selection_sets() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { profile: Profile }\ntype Profile { avatar: String, bio: String }",
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_path,
            r"query GetUser {
  user {
    profile {
      avatar
      bio
    }
  }
}",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let ranges = snapshot.folding_ranges(&query_path);

        // Should have multiple folding ranges for nested selection sets
        assert!(
            ranges.len() >= 2,
            "Should have multiple folding ranges, got: {ranges:?}"
        );
    }

    #[test]
    fn test_folding_ranges_fragment() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type User { id: ID!, name: String, email: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let fragment_path = FilePath::new("file:///fragment.graphql");
        host.add_file(
            &fragment_path,
            r"fragment UserFields on User {
  id
  name
  email
}",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let ranges = snapshot.folding_ranges(&fragment_path);

        // Should have folding range for the fragment
        assert!(!ranges.is_empty(), "Should have folding ranges");
        let has_fragment_fold = ranges.iter().any(|r| r.start_line == 0 && r.end_line == 4);
        assert!(
            has_fragment_fold,
            "Should have fragment folding range, got: {ranges:?}"
        );
    }

    #[test]
    fn test_folding_ranges_schema_types() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r"type User {
  id: ID!
  name: String!
  email: String
}

enum Status {
  ACTIVE
  INACTIVE
}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let ranges = snapshot.folding_ranges(&schema_path);

        // Should have folding ranges for type and enum definitions
        assert!(
            ranges.len() >= 2,
            "Should have folding ranges for type and enum, got: {ranges:?}"
        );
    }

    #[test]
    fn test_folding_ranges_block_comments() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#""""
This is a block comment
that spans multiple lines
"""
type User {
  id: ID!
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let ranges = snapshot.folding_ranges(&schema_path);

        // Should have a comment folding range
        let has_comment_fold = ranges.iter().any(|r| r.kind == FoldingRangeKind::Comment);
        assert!(
            has_comment_fold,
            "Should have comment folding range, got: {ranges:?}"
        );
    }

    #[test]
    fn test_folding_ranges_single_line_no_fold() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_path,
            r"query { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let ranges = snapshot.folding_ranges(&query_path);

        // Single line queries shouldn't have folding ranges
        assert!(
            ranges.is_empty(),
            "Single-line query should not have folding ranges, got: {ranges:?}"
        );
    }
}
