// Apollo-compiler validation integration
// This module provides comprehensive GraphQL validation using apollo-compiler

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use graphql_db::{FileContent, FileMetadata};
use std::sync::Arc;

/// Validate a document file using apollo-compiler
/// Returns apollo-compiler diagnostics converted to our Diagnostic type
///
/// This provides comprehensive validation including:
/// - Field selection validation against schema types
/// - Argument validation (required args, correct types)
/// - Fragment spread resolution and type checking
/// - Variable usage and type validation
/// - Circular fragment detection
/// - Type coercion validation
#[allow(clippy::too_many_lines)]
#[salsa::tracked]
pub fn validate_document(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: graphql_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    // Get the merged schema
    let Some(schema) = crate::merged_schema::merged_schema(db, project_files) else {
        // Without a schema, we can't validate documents
        // Return empty diagnostics (syntax errors are handled elsewhere)
        return Arc::new(diagnostics);
    };

    // Get the document text
    // For TypeScript/JavaScript files, we need to use the extracted GraphQL, not the full file
    let parse = graphql_syntax::parse(db, content, metadata);
    let doc_text: Arc<str> = if metadata.kind(db) == graphql_db::FileKind::TypeScript
        || metadata.kind(db) == graphql_db::FileKind::JavaScript
    {
        // Skip files with no GraphQL blocks
        if parse.blocks.is_empty() {
            return Arc::new(diagnostics);
        }

        // Combine all extracted GraphQL blocks into a single document
        let combined = parse
            .blocks
            .iter()
            .map(|block| block.source.as_ref())
            .collect::<Vec<_>>()
            .join("\n\n");
        Arc::from(combined)
    } else {
        // For pure GraphQL files, use the content as-is
        content.text(db)
    };

    let doc_uri = metadata.uri(db);

    // Collect fragment names referenced by this document (transitively across files)
    let document_files = project_files.document_files(db);
    let referenced_fragments =
        collect_referenced_fragments_transitive(&doc_text, &doc_uri, &document_files, db);

    // Use builder pattern to construct the executable document
    // This allows us to add fragments individually with proper source tracking
    let valid_schema = apollo_compiler::validation::Valid::assume_valid_ref(schema.as_ref());
    let mut errors = apollo_compiler::validation::DiagnosticList::new(Arc::default());
    let mut builder = apollo_compiler::ExecutableDocument::builder(Some(valid_schema), &mut errors);

    // Add the current document
    let line_offset_val = metadata.line_offset(db);
    let offset = apollo_compiler::parser::SourceOffset {
        line: (line_offset_val + 1) as usize, // Convert to 1-indexed
        column: 1,
    };

    apollo_compiler::parser::Parser::new()
        .source_offset(offset)
        .parse_into_executable_builder(doc_text.as_ref(), doc_uri.as_str(), &mut builder);

    // Add referenced fragments
    if !referenced_fragments.is_empty() {
        for (_file_id, file_content, file_metadata) in document_files.iter() {
            let text = file_content.text(db);
            let uri = file_metadata.uri(db);

            // Skip the current document (already included)
            if uri.as_str() == doc_uri.as_str() {
                continue;
            }

            // Check if this file defines any referenced fragments
            if file_defines_any_fragment(&text, &referenced_fragments) {
                // Extract GraphQL if this is a TypeScript/JavaScript file
                let fragment_text = if file_metadata.kind(db) == graphql_db::FileKind::TypeScript
                    || file_metadata.kind(db) == graphql_db::FileKind::JavaScript
                {
                    // Parse and extract GraphQL blocks
                    let fragment_parse = graphql_syntax::parse(db, *file_content, *file_metadata);
                    // Combine all extracted blocks
                    let combined = fragment_parse
                        .blocks
                        .iter()
                        .map(|block| block.source.as_ref())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    combined
                } else {
                    text.as_ref().to_string()
                };

                // Extract and add only the specific fragments we need
                apollo_compiler::parser::Parser::new().parse_into_executable_builder(
                    &fragment_text,
                    uri.as_str(),
                    &mut builder,
                );
            }
        }
    }

    // Build and validate
    let doc = builder.build();

    match if errors.is_empty() {
        doc.validate(valid_schema)
            .map(|_| ())
            .map_err(|with_errors| with_errors.errors)
    } else {
        Err(errors)
    } {
        Ok(_valid_document) => {
            // Document is valid
        }
        Err(error_list) => {
            // Convert apollo-compiler diagnostics to our format
            // Only include diagnostics for the current file

            // Iterate over the diagnostic list and filter to current file
            // Note: Since we used builder pattern, diagnostics will have proper source tracking
            // Note: apollo-compiler already returns line numbers adjusted for the source offset
            // we provided earlier, so we do NOT add line_offset here again
            #[allow(clippy::cast_possible_truncation, clippy::option_if_let_else)]
            for apollo_diag in error_list.iter() {
                // Filter diagnostics to only include those from the current file
                // This prevents duplicate reporting when fragments with errors are used in multiple files
                use apollo_compiler::diagnostic::ToCliReport;
                if let Some(location) = apollo_diag.error.location() {
                    let file_id = location.file_id();
                    // Get the file path for this diagnostic from the source map
                    if let Some(source_file) = apollo_diag.sources.get(&file_id) {
                        let diag_file_path = source_file.path();
                        // Only include diagnostics that belong to the current file
                        if diag_file_path != doc_uri.as_str() {
                            continue;
                        }
                    }
                }

                // Get location information if available
                // apollo-compiler returns 1-indexed line/column, we use 0-indexed
                // The SourceOffset we provided means positions are already in original file coordinates
                let range = if let Some(loc_range) = apollo_diag.line_column_range() {
                    DiagnosticRange {
                        start: Position {
                            line: loc_range.start.line.saturating_sub(1) as u32,
                            character: loc_range.start.column.saturating_sub(1) as u32,
                        },
                        end: Position {
                            line: loc_range.end.line.saturating_sub(1) as u32,
                            character: loc_range.end.column.saturating_sub(1) as u32,
                        },
                    }
                } else {
                    DiagnosticRange::default()
                };

                // Get message - apollo_diag.error is a GraphQLError which can be converted to string
                let message: Arc<str> = Arc::from(apollo_diag.error.to_string());

                // Filter out false positives: fragments are allowed to be standalone
                // and don't need to be used in operations (they may be used in other files)
                if message.contains("must be used in an operation") {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message,
                    range,
                    source: "apollo-compiler".into(),
                    code: None,
                });
            }
        }
    }

    Arc::new(diagnostics)
}

/// Collect all fragment names referenced by a document transitively across files
/// This resolves fragment dependencies by following fragment spreads to their definitions
fn collect_referenced_fragments_transitive(
    doc_text: &str,
    doc_uri: &graphql_db::FileUri,
    document_files: &Arc<Vec<(graphql_db::FileId, FileContent, FileMetadata)>>,
    db: &dyn GraphQLAnalysisDatabase,
) -> std::collections::HashSet<String> {
    use std::collections::{HashSet, VecDeque};

    // Start with fragments directly referenced in the current document
    let mut all_referenced = collect_referenced_fragments(doc_text);
    let mut to_process: VecDeque<String> = all_referenced.iter().cloned().collect();
    let mut processed = HashSet::new();

    // Process fragments transitively
    while let Some(fragment_name) = to_process.pop_front() {
        if processed.contains(&fragment_name) {
            continue;
        }
        processed.insert(fragment_name.clone());

        // Find the file that defines this fragment
        for (_file_id, file_content, file_metadata) in document_files.iter() {
            let text = file_content.text(db);
            let uri = file_metadata.uri(db);

            // Skip the current document
            if uri.as_str() == doc_uri.as_str() {
                continue;
            }

            // Check if this file defines the fragment
            if let Some(fragment_refs) = get_fragment_references(&text, &fragment_name) {
                // This file defines the fragment
                // Collect fragments that THIS fragment references
                for ref_name in fragment_refs {
                    if all_referenced.insert(ref_name.clone()) {
                        // New fragment found, add to processing queue
                        to_process.push_back(ref_name);
                    }
                }
                break; // Found the definition, no need to check other files
            }
        }
    }

    all_referenced
}

/// Collect all fragment names referenced by a document (in the same file only)
/// This includes fragments directly referenced in operations and fragments referenced by other fragments
fn collect_referenced_fragments(text: &str) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    let parser = apollo_parser::Parser::new(text);
    let tree = parser.parse();

    if tree.errors().next().is_some() {
        // If there are parse errors, return empty set (apollo-compiler will report the errors)
        return HashSet::new();
    }

    let mut referenced = HashSet::new();
    let document = tree.document();

    // Collect all fragment spreads from operations and fragments
    for definition in document.definitions() {
        match definition {
            apollo_parser::cst::Definition::OperationDefinition(op) => {
                collect_fragment_spreads_from_selection_set(op.selection_set(), &mut referenced);
            }
            apollo_parser::cst::Definition::FragmentDefinition(frag) => {
                collect_fragment_spreads_from_selection_set(frag.selection_set(), &mut referenced);
            }
            _ => {}
        }
    }

    referenced
}

/// Get the fragment names referenced by a specific fragment definition in a file
/// Returns None if the fragment is not defined in this file
fn get_fragment_references(text: &str, fragment_name: &str) -> Option<Vec<String>> {
    let parser = apollo_parser::Parser::new(text);
    let tree = parser.parse();

    if tree.errors().next().is_some() {
        return None;
    }

    let document = tree.document();

    for definition in document.definitions() {
        if let apollo_parser::cst::Definition::FragmentDefinition(frag) = definition {
            if let Some(name) = frag.fragment_name() {
                if let Some(name_node) = name.name() {
                    if name_node.text() == fragment_name {
                        // Found the fragment definition, collect its references
                        let mut referenced = std::collections::HashSet::new();
                        collect_fragment_spreads_from_selection_set(
                            frag.selection_set(),
                            &mut referenced,
                        );
                        return Some(referenced.into_iter().collect());
                    }
                }
            }
        }
    }

    None
}

/// Recursively collect fragment spreads from a selection set
fn collect_fragment_spreads_from_selection_set(
    selection_set: Option<apollo_parser::cst::SelectionSet>,
    referenced: &mut std::collections::HashSet<String>,
) {
    let Some(selection_set) = selection_set else {
        return;
    };

    for selection in selection_set.selections() {
        match selection {
            apollo_parser::cst::Selection::Field(field) => {
                // Recurse into nested selection sets
                collect_fragment_spreads_from_selection_set(field.selection_set(), referenced);
            }
            apollo_parser::cst::Selection::FragmentSpread(spread) => {
                // Add fragment name to referenced set
                if let Some(name) = spread.fragment_name() {
                    if let Some(name_node) = name.name() {
                        referenced.insert(name_node.text().to_string());
                    }
                }
            }
            apollo_parser::cst::Selection::InlineFragment(inline_frag) => {
                // Recurse into inline fragment selection set
                collect_fragment_spreads_from_selection_set(
                    inline_frag.selection_set(),
                    referenced,
                );
            }
        }
    }
}

/// Check if a file defines any of the given fragment names
fn file_defines_any_fragment(
    text: &str,
    fragment_names: &std::collections::HashSet<String>,
) -> bool {
    let parser = apollo_parser::Parser::new(text);
    let tree = parser.parse();

    if tree.errors().next().is_some() {
        // If there are parse errors, skip this file
        return false;
    }

    let document = tree.document();

    for definition in document.definitions() {
        if let apollo_parser::cst::Definition::FragmentDefinition(frag) = definition {
            if let Some(name) = frag.fragment_name() {
                if let Some(name_node) = name.name() {
                    let frag_name = name_node.text();
                    let frag_name_str = frag_name.as_str();
                    if fragment_names.contains(frag_name_str) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileId, FileKind, FileUri, ProjectFiles};
    use salsa::Setter;

    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

    #[salsa::db]
    impl graphql_hir::GraphQLHirDatabase for TestDatabase {}

    #[salsa::db]
    impl crate::GraphQLAnalysisDatabase for TestDatabase {}

    #[test]
    fn test_validate_document_no_schema() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        let content = FileContent::new(&db, Arc::from("query { hello }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Empty project files (no schema)
        let project_files = ProjectFiles::new(&db, Arc::new(vec![]), Arc::new(vec![]));

        let diagnostics = validate_document(&db, content, metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics when schema is missing"
        );
    }

    #[test]
    fn test_validate_document_with_valid_fragment() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content =
            FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with only fragment definitions (no operations)
        // This is a valid document - fragments don't need to be used in operations
        // within the same file, they may be used in other files
        let doc_id = FileId::new(1);
        let doc_content =
            FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics for valid fragment. Fragments don't need operations in the same file."
        );
    }

    #[test]
    fn test_validate_document_with_invalid_fragment() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content =
            FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with fragment that has an invalid field
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(
            &db,
            Arc::from("fragment UserFields on User { invalidField }"),
        );
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for fragment with invalid field"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("invalidField") || d.message.contains("field")),
            "Expected error message about invalid field 'invalidField'"
        );
    }

    #[test]
    fn test_validate_document_invalid_field() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with invalid field
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { world }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for invalid field selection"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("world") || d.message.contains("field")),
            "Expected error message about invalid field 'world'"
        );
    }

    #[test]
    fn test_validate_document_valid_query() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create valid document
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { hello }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics for valid query"
        );
    }

    #[test]
    fn test_validate_document_missing_required_argument() {
        let db = TestDatabase::default();

        // Create schema with required argument
        let schema_id = FileId::new(0);
        let schema_content =
            FileContent::new(&db, Arc::from("type Query { user(id: ID!): String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document missing required argument
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { user }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for missing required argument"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("id") || d.message.contains("argument")),
            "Expected error message about missing argument 'id'"
        );
    }

    #[test]
    fn test_validate_document_invalid_variable_type() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with invalid variable type
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query($var: UnknownType) { hello }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for invalid variable type"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("UnknownType") || d.message.contains("type")),
            "Expected error message about unknown type 'UnknownType'"
        );
    }

    #[test]
    fn test_cross_file_fragment_resolution() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from("type Query { user: User } type User { id: ID! name: String! }"),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create fragment file
        let frag_id = FileId::new(1);
        let frag_content =
            FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
        let frag_metadata = FileMetadata::new(
            &db,
            frag_id,
            FileUri::new("fragments.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Create query that uses the fragment from another file
        let query_id = FileId::new(2);
        let query_content = FileContent::new(&db, Arc::from("query { user { ...UserFields } }"));
        let query_metadata = FileMetadata::new(
            &db,
            query_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![
                (frag_id, frag_content, frag_metadata),
                (query_id, query_content, query_metadata),
            ]),
        );

        // Validate the query - it should find the fragment from the other file
        let diagnostics = validate_document(&db, query_content, query_metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics when fragment is defined in another file. Got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_line_offset_adjustment() {
        let mut db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with invalid query and line_offset of 10
        // This simulates extracted GraphQL from TypeScript/JavaScript at line 10
        // The content is already extracted GraphQL, so we mark it as ExecutableGraphQL
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { invalidField }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.ts"),
            FileKind::ExecutableGraphQL,
        );
        // Set line offset to simulate extraction from line 10 in TypeScript file
        doc_metadata.set_line_offset(&mut db).to(10);

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_metadata)]),
            Arc::new(vec![(doc_id, doc_content, doc_metadata)]),
        );

        let diagnostics = validate_document(&db, doc_content, doc_metadata, project_files);
        assert!(!diagnostics.is_empty(), "Expected validation errors");

        // Verify that line numbers are adjusted by the line offset
        for diag in diagnostics.iter() {
            // The error should be at line 10 or later (accounting for the offset)
            // The GraphQL error is at line 0 in the extracted text, but should be reported at line 10
            assert!(
                diag.range.start.line >= 10,
                "Expected diagnostic line to be adjusted by offset. Got line: {}",
                diag.range.start.line
            );
        }
    }
}
