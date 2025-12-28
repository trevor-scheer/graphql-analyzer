// Merged schema query for apollo-compiler validation
// This module provides a Salsa query that aggregates all schema files
// into a single apollo_compiler::Schema for validation purposes.

use crate::GraphQLAnalysisDatabase;
use apollo_compiler::parser::Parser;
use std::sync::Arc;

/// Get the merged apollo-compiler Schema for validation
/// This is expensive but heavily cached by Salsa
///
/// Returns None if:
/// - No schema files exist in the project
/// - Schema files fail to parse
/// - Schema merging fails
#[salsa::tracked]
pub fn merged_schema(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Option<Arc<apollo_compiler::Schema>> {
    tracing::info!("merged_schema: Starting schema merge");
    let schema_files = project_files.schema_files(db);
    tracing::info!(schema_file_count = schema_files.len(), "Found schema files");

    if schema_files.is_empty() {
        tracing::info!("No schema files found in project - returning None");
        return None;
    }

    // Use apollo-compiler's builder pattern to parse multiple schema files
    let mut builder = apollo_compiler::schema::SchemaBuilder::new();
    let mut parser = Parser::new();

    // Parse each schema file separately so apollo-compiler tracks sources correctly
    for (_file_id, content, metadata) in schema_files.iter() {
        let text = content.text(db);
        let uri = metadata.uri(db);

        tracing::debug!(uri = ?uri, "Adding schema file to merge");

        // Parse and add to builder
        parser.parse_into_schema_builder(text.as_ref(), uri.as_str(), &mut builder);
    }

    // Build and validate the schema
    match builder.build() {
        Ok(schema) => {
            tracing::debug!(
                type_count = schema.types.len(),
                "Successfully merged schema"
            );
            Some(Arc::new(schema))
        }
        Err(with_errors) => {
            tracing::warn!(
                error_count = with_errors.errors.len(),
                "Failed to merge schema due to validation errors"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};

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
    fn test_merged_schema_single_file() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(file_id, content, metadata)]),
            Arc::new(vec![]),
        );

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_some(),
            "Expected schema to be merged successfully"
        );

        let schema = schema.unwrap();
        assert!(
            schema.types.contains_key("Query"),
            "Expected Query type to exist in merged schema"
        );
    }

    #[test]
    fn test_merged_schema_multiple_files() {
        let db = TestDatabase::default();

        let file1_id = FileId::new(0);
        let content1 = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata1 = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("schema1.graphql"),
            FileKind::Schema,
        );

        let file2_id = FileId::new(1);
        let content2 = FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
        let metadata2 = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("schema2.graphql"),
            FileKind::Schema,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![
                (file1_id, content1, metadata1),
                (file2_id, content2, metadata2),
            ]),
            Arc::new(vec![]),
        );

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_some(),
            "Expected schema to be merged successfully"
        );

        let schema = schema.unwrap();
        assert!(
            schema.types.contains_key("Query"),
            "Expected Query type to exist in merged schema"
        );
        assert!(
            schema.types.contains_key("User"),
            "Expected User type to exist in merged schema"
        );
    }

    #[test]
    fn test_merged_schema_with_extensions() {
        let db = TestDatabase::default();

        let file1_id = FileId::new(0);
        let content1 = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata1 = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("schema1.graphql"),
            FileKind::Schema,
        );

        let file2_id = FileId::new(1);
        let content2 = FileContent::new(&db, Arc::from("extend type Query { world: String }"));
        let metadata2 = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("schema2.graphql"),
            FileKind::Schema,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![
                (file1_id, content1, metadata1),
                (file2_id, content2, metadata2),
            ]),
            Arc::new(vec![]),
        );

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_some(),
            "Expected schema to be merged successfully"
        );

        let schema = schema.unwrap();
        let query_type = schema.types.get("Query");
        assert!(query_type.is_some(), "Expected Query type to exist");

        // Both hello and world fields should be present
        if let Some(apollo_compiler::schema::ExtendedType::Object(obj)) = query_type {
            let field_names: Vec<&str> = obj
                .fields
                .keys()
                .map(apollo_compiler::Name::as_str)
                .collect();
            assert!(
                field_names.contains(&"hello"),
                "Expected hello field in Query type"
            );
            assert!(
                field_names.contains(&"world"),
                "Expected world field in Query type (from extension)"
            );
        } else {
            panic!("Expected Query to be an object type");
        }
    }

    #[test]
    fn test_merged_schema_no_files() {
        let db = TestDatabase::default();

        let project_files = ProjectFiles::new(&db, Arc::new(vec![]), Arc::new(vec![]));

        let schema = merged_schema(&db, project_files);
        assert!(schema.is_none(), "Expected None when no schema files exist");
    }

    #[test]
    fn test_merged_schema_invalid_syntax() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        let content = FileContent::new(&db, Arc::from("type Query { invalid syntax here"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(file_id, content, metadata)]),
            Arc::new(vec![]),
        );

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_none(),
            "Expected None when schema has parse errors"
        );
    }

    #[test]
    fn test_merged_schema_validation_error() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        // Valid syntax but invalid semantics (duplicate type definition)
        let content = FileContent::new(
            &db,
            Arc::from("type Query { hello: String }\ntype Query { world: String }"),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(file_id, content, metadata)]),
            Arc::new(vec![]),
        );

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_none(),
            "Expected None when schema has validation errors"
        );
    }
}
