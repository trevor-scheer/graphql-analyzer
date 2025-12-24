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
#[salsa::tracked]
pub fn validate_document(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: graphql_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    tracing::info!("validate_document: Starting validation");
    let mut diagnostics = Vec::new();

    // Get the merged schema
    let Some(schema) = crate::merged_schema::merged_schema(db, project_files) else {
        tracing::info!("No schema available for document validation - cannot validate");
        // Without a schema, we can't validate documents
        // Return empty diagnostics (syntax errors are handled elsewhere)
        return Arc::new(diagnostics);
    };
    tracing::info!("Merged schema obtained successfully");

    // Get the document text
    let doc_text = content.text(db);
    let doc_uri = metadata.uri(db);
    tracing::info!(
        uri = ?doc_uri,
        doc_length = doc_text.len(),
        "Document info"
    );

    // Check if this is a fragment-only document
    // Fragment-only documents should not be validated as executable documents
    let is_fragment_only = is_fragment_only_document(&doc_text);
    tracing::info!(is_fragment_only, "Checked if document is fragment-only");
    if is_fragment_only {
        tracing::info!("Skipping validation for fragment-only document");
        return Arc::new(diagnostics);
    }

    // Collect fragment names referenced by this document (transitively across files)
    let document_files = project_files.document_files(db);
    let referenced_fragments =
        collect_referenced_fragments_transitive(&doc_text, &doc_uri, &document_files, db);

    tracing::debug!(
        fragment_count = referenced_fragments.len(),
        "Found referenced fragments (transitive)"
    );

    // Build a combined document with the current document + referenced fragments
    let combined_doc = if referenced_fragments.is_empty() {
        // No external fragments referenced, just use the current document
        doc_text.to_string()
    } else {
        // Collect fragment definitions from other files
        let mut combined = String::from(doc_text.as_ref());

        for (_file_id, file_content, file_metadata) in document_files.iter() {
            let text = file_content.text(db);
            let uri = file_metadata.uri(db);

            // Skip the current document (already included)
            if uri.as_str() == doc_uri.as_str() {
                continue;
            }

            // Check if this file defines any referenced fragments
            if file_defines_any_fragment(&text, &referenced_fragments) {
                combined.push_str("\n\n");
                combined.push_str(&text);
            }
        }

        combined
    };

    // Parse and validate the combined document with apollo-compiler
    // Wrap the schema in Valid since we got it from merged_schema which validates it
    tracing::info!(
        combined_doc_length = combined_doc.len(),
        "About to call apollo-compiler validation"
    );
    let valid_schema = apollo_compiler::validation::Valid::assume_valid_ref(schema.as_ref());
    match apollo_compiler::ExecutableDocument::parse_and_validate(
        valid_schema,
        combined_doc,
        doc_uri.as_str(),
    ) {
        Ok(_valid_document) => {
            // Document is valid
            tracing::info!("Document validated successfully - no errors");
        }
        Err(with_errors) => {
            // Convert apollo-compiler diagnostics to our format
            // Only include diagnostics for the current file
            let error_list = &with_errors.errors;
            tracing::info!(
                error_count = error_list.len(),
                "Apollo-compiler found validation errors"
            );

            // Get line offset for TypeScript/JavaScript extraction
            let line_offset = metadata.line_offset(db);

            // Iterate over the diagnostic list and filter to current file
            // Note: Since we combined documents, all diagnostics will be relative to the doc_uri
            // Apollo-compiler tracks sources correctly when parsing, so diagnostics will have proper locations
            #[allow(clippy::cast_possible_truncation, clippy::option_if_let_else)]
            for apollo_diag in error_list.iter() {
                // Get location information if available
                let range = if let Some(loc_range) = apollo_diag.line_column_range() {
                    DiagnosticRange {
                        start: Position {
                            // apollo-compiler uses 1-indexed, we use 0-indexed
                            // Casting usize to u32 is safe for line/column numbers in practice
                            // Add line_offset to adjust for TypeScript/JavaScript extraction
                            line: loc_range.start.line.saturating_sub(1) as u32 + line_offset,
                            character: loc_range.start.column.saturating_sub(1) as u32,
                        },
                        end: Position {
                            line: loc_range.end.line.saturating_sub(1) as u32 + line_offset,
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

            tracing::debug!(
                diagnostic_count = diagnostics.len(),
                "Document validation found errors for current file"
            );
        }
    }

    Arc::new(diagnostics)
}

/// Check if a document contains only fragment definitions (no operations)
/// Fragment-only documents are valid but should not be validated as executable documents
fn is_fragment_only_document(text: &str) -> bool {
    // Use apollo-parser to check the structure
    let parser = apollo_parser::Parser::new(text);
    let tree = parser.parse();

    if tree.errors().next().is_some() {
        // If there are parse errors, let apollo-compiler handle validation
        return false;
    }

    let document = tree.document();
    let mut has_operation = false;
    let mut has_fragment = false;

    for definition in document.definitions() {
        match definition {
            apollo_parser::cst::Definition::OperationDefinition(_) => {
                has_operation = true;
            }
            apollo_parser::cst::Definition::FragmentDefinition(_) => {
                has_fragment = true;
            }
            _ => {}
        }
    }

    // Fragment-only if it has fragments but no operations
    has_fragment && !has_operation
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
                    if fragment_names.contains(name_node.text().as_str()) {
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
    fn test_validate_document_fragment_only() {
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

        // Create fragment-only document
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
            "Expected no diagnostics for fragment-only document"
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
    fn test_is_fragment_only_document() {
        assert!(is_fragment_only_document(
            "fragment UserFields on User { id }"
        ));
        assert!(is_fragment_only_document(
            "fragment A on User { id } fragment B on Post { title }"
        ));
        assert!(!is_fragment_only_document("query { hello }"));
        assert!(!is_fragment_only_document(
            "query { hello } fragment F on User { id }"
        ));
        assert!(!is_fragment_only_document("mutation { updateUser }"));
        assert!(!is_fragment_only_document("invalid syntax here"));
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
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { invalidField }"));
        let doc_metadata =
            FileMetadata::new(&db, doc_id, FileUri::new("query.ts"), FileKind::TypeScript);
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
