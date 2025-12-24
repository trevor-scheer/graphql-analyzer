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
    let mut diagnostics = Vec::new();

    // Get the merged schema
    let Some(schema) = crate::merged_schema::merged_schema(db, project_files) else {
        tracing::debug!("No schema available for document validation");
        // Without a schema, we can't validate documents
        // Return empty diagnostics (syntax errors are handled elsewhere)
        return Arc::new(diagnostics);
    };

    // Get the document text
    let doc_text = content.text(db);

    // Check if this is a fragment-only document
    // Fragment-only documents should not be validated as executable documents
    if is_fragment_only_document(&doc_text) {
        tracing::debug!("Skipping validation for fragment-only document");
        return Arc::new(diagnostics);
    }

    // Parse and validate the document with apollo-compiler
    // Wrap the schema in Valid since we got it from merged_schema which validates it
    let valid_schema = apollo_compiler::validation::Valid::assume_valid_ref(schema.as_ref());
    match apollo_compiler::ExecutableDocument::parse_and_validate(
        valid_schema,
        doc_text.as_ref(),
        metadata.uri(db).as_str(),
    ) {
        Ok(_valid_document) => {
            // Document is valid
            tracing::debug!("Document validated successfully");
        }
        Err(with_errors) => {
            // Convert apollo-compiler diagnostics to our format
            let error_list = &with_errors.errors;
            let error_count = error_list.len();

            // Iterate over the diagnostic list and convert each diagnostic
            #[allow(clippy::cast_possible_truncation, clippy::option_if_let_else)]
            for apollo_diag in error_list.iter() {
                // Get location information if available
                let range = if let Some(loc_range) = apollo_diag.line_column_range() {
                    DiagnosticRange {
                        start: Position {
                            // apollo-compiler uses 1-indexed, we use 0-indexed
                            // Casting usize to u32 is safe for line/column numbers in practice
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

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message,
                    range,
                    source: "apollo-compiler".into(),
                    code: None,
                });
            }

            tracing::debug!(error_count, "Document validation found errors");
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

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileId, FileKind, FileUri, ProjectFiles};

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
}
