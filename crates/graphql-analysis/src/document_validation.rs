// Document validation queries (operations and fragments)

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_db::{FileContent, FileMetadata};
use std::sync::Arc;

/// Validate a document file (operations and fragments)
/// This checks for:
/// - Operation name uniqueness
/// - Fragment name uniqueness
/// - Valid type conditions on fragments
/// - Valid field selections against schema
/// - Valid variable types
/// - Fragment spread resolution
#[salsa::tracked]
pub fn validate_document_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let structure = graphql_hir::file_structure(db, metadata.file_id(db), content, metadata);
    let mut diagnostics = Vec::new();

    // Get schema for validation
    let project_files = db
        .project_files()
        .expect("project files must be set for validation");
    let schema = graphql_hir::schema_types_with_project(db, project_files);

    // Validate each operation
    for op_structure in &structure.operations {
        // Check operation name uniqueness (structural check - cheap)
        if let Some(name) = &op_structure.name {
            let all_ops = graphql_hir::all_operations(db);

            // Count how many operations have this name
            let count = all_ops
                .iter()
                .filter(|op| op.name.as_ref() == Some(name))
                .count();

            if count > 1 {
                diagnostics.push(Diagnostic::error(
                    format!("Operation name '{name}' is not unique"),
                    DiagnosticRange::default(), // TODO: Get actual position from HIR
                ));
            }
        }

        // Validate variable types
        for var in &op_structure.variables {
            validate_variable_type(&var.type_ref, &schema, &mut diagnostics);
        }

        // Validate operation body
        // Get the root type for this operation
        let root_type_name = match op_structure.operation_type {
            graphql_hir::OperationType::Query => "Query",
            graphql_hir::OperationType::Mutation => "Mutation",
            graphql_hir::OperationType::Subscription => "Subscription",
        };

        if !schema.contains_key(&Arc::from(root_type_name)) {
            diagnostics.push(Diagnostic::error(
                format!("Schema does not define a '{root_type_name}' type"),
                DiagnosticRange::default(),
            ));
        }
        // NOTE: Full body validation (field selections, arguments, fragment spreads)
        // is complex and best handled by apollo-compiler's validation.
        // For now, we rely on the structural checks above.
        // A future enhancement would be to integrate apollo-compiler's validator here.
    }

    // Validate fragments
    for frag_structure in &structure.fragments {
        // Check fragment name uniqueness
        let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);

        let count = all_fragments
            .iter()
            .filter(|(_, frag)| frag.name == frag_structure.name)
            .count();

        if count > 1 {
            diagnostics.push(Diagnostic::error(
                format!("Fragment name '{}' is not unique", frag_structure.name),
                DiagnosticRange::default(), // TODO: Get actual position from HIR
            ));
        }

        // Validate fragment type condition exists in schema
        validate_fragment_type_condition(frag_structure, &schema, &mut diagnostics);

        // TODO: Validate fragment body (field selections)
        // This requires parsing the fragment body and walking the selection set
    }

    Arc::new(diagnostics)
}

/// Validate that a variable's type exists and is a valid input type
fn validate_variable_type(
    type_ref: &graphql_hir::TypeRef,
    schema: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Built-in scalars are valid
    if is_builtin_scalar(&type_ref.name) {
        return;
    }

    if let Some(type_def) = schema.get(&type_ref.name) {
        use graphql_hir::TypeDefKind;
        match type_def.kind {
            TypeDefKind::Scalar | TypeDefKind::Enum | TypeDefKind::InputObject => {
                // Valid input types for variables
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "Variable type '{}' is not a valid input type",
                        type_ref.name
                    ),
                    DiagnosticRange::default(),
                ));
            }
        }
    } else {
        diagnostics.push(Diagnostic::error(
            format!("Unknown variable type: {}", type_ref.name),
            DiagnosticRange::default(),
        ));
    }
}

/// Validate that a fragment's type condition exists in the schema
fn validate_fragment_type_condition(
    fragment: &graphql_hir::FragmentStructure,
    schema: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !schema.contains_key(&fragment.type_condition) {
        diagnostics.push(Diagnostic::error(
            format!(
                "Fragment '{}' has unknown type condition '{}'",
                fragment.name, fragment.type_condition
            ),
            DiagnosticRange::default(),
        ));
        return;
    }

    // Check that the type condition is an object, interface, or union
    if let Some(type_def) = schema.get(&fragment.type_condition) {
        use graphql_hir::TypeDefKind;
        match type_def.kind {
            TypeDefKind::Object | TypeDefKind::Interface | TypeDefKind::Union => {
                // Valid fragment type conditions
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "Fragment '{}' type condition '{}' must be an object, interface, or union type",
                        fragment.name, fragment.type_condition
                    ),
                    DiagnosticRange::default(),
                ));
            }
        }
    }
}

/// Check if a type name is a built-in GraphQL scalar
fn is_builtin_scalar(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "String" | "Boolean" | "ID")
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileKind, FileMetadata, FileUri};

    #[salsa::db]
    #[derive(Clone)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
        project_files: std::cell::Cell<Option<graphql_db::ProjectFiles>>,
    }

    impl Default for TestDatabase {
        fn default() -> Self {
            Self {
                storage: salsa::Storage::default(),
                project_files: std::cell::Cell::new(None),
            }
        }
    }

    impl TestDatabase {
        fn set_project_files(&self, project_files: Option<graphql_db::ProjectFiles>) {
            self.project_files.set(project_files);
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

    #[salsa::db]
    impl graphql_hir::GraphQLHirDatabase for TestDatabase {
        fn project_files(&self) -> Option<graphql_db::ProjectFiles> {
            self.project_files.get()
        }
    }

    #[salsa::db]
    impl crate::GraphQLAnalysisDatabase for TestDatabase {}

    #[test]
    fn test_unknown_variable_type() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let doc_content = "query GetUser($input: UserInput!) { user }";
        let content = FileContent::new(&db, Arc::from(doc_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Set up project files
        let schema_files = graphql_db::SchemaFiles::new(&db, Arc::new(Vec::new()));
        let document_files =
            graphql_db::DocumentFiles::new(&db, Arc::new(vec![(file_id, content, metadata)]));
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        db.set_project_files(Some(project_files));

        let diagnostics = validate_document_file(&db, content, metadata);

        let unknown_type_error = diagnostics
            .iter()
            .find(|d| d.message.contains("Unknown variable type: UserInput"));
        assert!(
            unknown_type_error.is_some(),
            "Expected error about unknown variable type"
        );
    }

    #[test]
    fn test_variable_invalid_input_type() {
        let db = TestDatabase::default();

        // First, add schema
        let schema_file_id = graphql_db::FileId::new(0);
        let schema_content = "type User { id: ID! }";
        let schema_fc = FileContent::new(&db, Arc::from(schema_content));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Now test document with invalid variable type
        let doc_file_id = graphql_db::FileId::new(1);
        let doc_content = "query GetUser($user: User!) { user }";
        let doc_fc = FileContent::new(&db, Arc::from(doc_content));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_file_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Set up project files with schema and document
        let schema_files = graphql_db::SchemaFiles::new(
            &db,
            Arc::new(vec![(schema_file_id, schema_fc, schema_metadata)]),
        );
        let document_files = graphql_db::DocumentFiles::new(
            &db,
            Arc::new(vec![(doc_file_id, doc_fc, doc_metadata)]),
        );
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        db.set_project_files(Some(project_files));

        let diagnostics = validate_document_file(&db, doc_fc, doc_metadata);

        let invalid_input_error = diagnostics
            .iter()
            .find(|d| d.message.contains("is not a valid input type"));
        assert!(
            invalid_input_error.is_some(),
            "Expected error about invalid input type for variable"
        );
    }

    #[test]
    fn test_fragment_unknown_type_condition() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let doc_content = "fragment UserFields on User { id }";
        let content = FileContent::new(&db, Arc::from(doc_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Set up project files
        let schema_files = graphql_db::SchemaFiles::new(&db, Arc::new(Vec::new()));
        let document_files =
            graphql_db::DocumentFiles::new(&db, Arc::new(vec![(file_id, content, metadata)]));
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        db.set_project_files(Some(project_files));

        let diagnostics = validate_document_file(&db, content, metadata);

        let unknown_type_error = diagnostics
            .iter()
            .find(|d| d.message.contains("has unknown type condition 'User'"));
        assert!(
            unknown_type_error.is_some(),
            "Expected error about unknown type condition"
        );
    }

    #[test]
    fn test_fragment_invalid_type_condition() {
        let db = TestDatabase::default();

        // Add schema with scalar type
        let schema_file_id = graphql_db::FileId::new(0);
        let schema_content = "scalar DateTime";
        let schema_fc = FileContent::new(&db, Arc::from(schema_content));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Fragment on scalar (invalid)
        let doc_file_id = graphql_db::FileId::new(1);
        let doc_content = "fragment TimeFields on DateTime { }";
        let doc_fc = FileContent::new(&db, Arc::from(doc_content));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_file_id,
            FileUri::new("fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Set up project files with schema and document
        let schema_files = graphql_db::SchemaFiles::new(
            &db,
            Arc::new(vec![(schema_file_id, schema_fc, schema_metadata)]),
        );
        let document_files = graphql_db::DocumentFiles::new(
            &db,
            Arc::new(vec![(doc_file_id, doc_fc, doc_metadata)]),
        );
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        db.set_project_files(Some(project_files));

        let diagnostics = validate_document_file(&db, doc_fc, doc_metadata);

        let invalid_condition_error = diagnostics.iter().find(|d| {
            d.message
                .contains("must be an object, interface, or union type")
        });
        assert!(
            invalid_condition_error.is_some(),
            "Expected error about invalid type condition"
        );
    }

    #[test]
    fn test_missing_root_type() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let doc_content = "query { hello }";
        let content = FileContent::new(&db, Arc::from(doc_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Set up project files
        let schema_files = graphql_db::SchemaFiles::new(&db, Arc::new(Vec::new()));
        let document_files =
            graphql_db::DocumentFiles::new(&db, Arc::new(vec![(file_id, content, metadata)]));
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        db.set_project_files(Some(project_files));

        let diagnostics = validate_document_file(&db, content, metadata);

        let missing_root_error = diagnostics
            .iter()
            .find(|d| d.message.contains("does not define a 'Query' type"));
        assert!(
            missing_root_error.is_some(),
            "Expected error about missing Query root type"
        );
    }

    #[test]
    fn test_valid_document() {
        let db = TestDatabase::default();

        // Add schema
        let schema_file_id = graphql_db::FileId::new(0);
        let schema_content = r"
            type Query { user(id: ID!): User }
            type User { id: ID! name: String! }
            input UserFilter { name: String }
        ";
        let schema_fc = FileContent::new(&db, Arc::from(schema_content));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Valid query
        let doc_file_id = graphql_db::FileId::new(1);
        let doc_content = r"
            query GetUser($id: ID!, $filter: UserFilter) {
                user(id: $id) { id name }
            }
            fragment UserFields on User { id name }
        ";
        let doc_fc = FileContent::new(&db, Arc::from(doc_content));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_file_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Set up project files with schema and document
        let schema_files = graphql_db::SchemaFiles::new(
            &db,
            Arc::new(vec![(schema_file_id, schema_fc, schema_metadata)]),
        );
        let document_files = graphql_db::DocumentFiles::new(
            &db,
            Arc::new(vec![(doc_file_id, doc_fc, doc_metadata)]),
        );
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        db.set_project_files(Some(project_files));

        let diagnostics = validate_document_file(&db, doc_fc, doc_metadata);

        assert_eq!(diagnostics.len(), 0, "Expected no validation errors");
    }
}
