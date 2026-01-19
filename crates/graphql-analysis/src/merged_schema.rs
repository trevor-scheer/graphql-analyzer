use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use apollo_compiler::parser::Parser;
use apollo_compiler::validation::DiagnosticList;
use std::sync::Arc;

/// Convert apollo-compiler diagnostics to our diagnostic format
#[allow(clippy::cast_possible_truncation)]
fn collect_apollo_diagnostics(errors: &DiagnosticList) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for apollo_diag in errors.iter() {
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

        let message: Arc<str> = Arc::from(apollo_diag.error.to_string());

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message,
            range,
            source: "apollo-compiler".into(),
            code: None,
        });
    }

    diagnostics
}

/// Result of merging schema files - includes both the schema (if valid) and any diagnostics
#[derive(Clone, Debug, PartialEq)]
pub struct MergedSchemaResult {
    /// The merged schema, if validation succeeded
    pub schema: Option<Arc<apollo_compiler::Schema>>,
    /// Validation diagnostics from the merge process
    pub diagnostics: Arc<Vec<Diagnostic>>,
}

/// Merge all schema files into a single `apollo_compiler::Schema` and collect validation errors
/// This query depends ONLY on schema file IDs and their content, not `DocumentFiles`.
/// Changing document files will not invalidate this query.
///
/// This function now performs full validation including:
/// - Interface implementation validation (types must implement all interface fields)
/// - Union member validation (union members must be object types)
/// - Type reference validation
#[salsa::tracked]
pub fn merged_schema_with_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> MergedSchemaResult {
    tracing::info!("merged_schema: Starting schema merge with diagnostics");
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    tracing::info!(schema_file_count = schema_ids.len(), "Found schema files");

    if schema_ids.is_empty() {
        tracing::info!("No schema files found in project - returning empty result");
        return MergedSchemaResult {
            schema: None,
            diagnostics: Arc::new(vec![]),
        };
    }

    let mut builder = apollo_compiler::schema::SchemaBuilder::new();
    let mut parser = Parser::new();

    for file_id in schema_ids.iter() {
        // Use per-file lookup for granular caching
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };
        let text = content.text(db);
        let uri = metadata.uri(db);

        tracing::debug!(uri = ?uri, "Adding schema file to merge");

        // Parse and add to builder
        parser.parse_into_schema_builder(text.as_ref(), uri.as_str(), &mut builder);
    }

    match builder.build() {
        Ok(schema) => {
            // SchemaBuilder::build() is lenient - it succeeds even with validation errors.
            // We call validate() to catch semantic issues like:
            // - Missing interface field implementations
            // - Union members that aren't object types
            // - Invalid type references
            //
            // IMPORTANT: We still return the schema even if validation fails, because
            // we need it for document validation. A schema without a Query type or with
            // minor issues should still allow fragment and operation validation.
            match schema.validate() {
                Ok(valid_schema) => {
                    tracing::debug!(
                        type_count = valid_schema.types.len(),
                        "Successfully merged and validated schema"
                    );
                    MergedSchemaResult {
                        schema: Some(Arc::new(valid_schema.into_inner())),
                        diagnostics: Arc::new(vec![]),
                    }
                }
                Err(with_errors) => {
                    tracing::warn!(
                        error_count = with_errors.errors.len(),
                        "Schema validation errors found (schema still usable for document validation)"
                    );
                    let diagnostics = collect_apollo_diagnostics(&with_errors.errors);
                    // Return the schema even with validation errors so document validation can proceed
                    MergedSchemaResult {
                        schema: Some(Arc::new(with_errors.partial)),
                        diagnostics: Arc::new(diagnostics),
                    }
                }
            }
        }
        Err(with_errors) => {
            tracing::warn!(
                error_count = with_errors.errors.len(),
                "Failed to merge schema due to build errors"
            );
            let diagnostics = collect_apollo_diagnostics(&with_errors.errors);
            MergedSchemaResult {
                schema: None,
                diagnostics: Arc::new(diagnostics),
            }
        }
    }
}

/// Merge all schema files into a single `apollo_compiler::Schema`
/// This query depends ONLY on schema file IDs and their content, not `DocumentFiles`.
/// Changing document files will not invalidate this query.
///
/// **Note**: This function discards validation diagnostics. If you need schema
/// validation errors, use [`merged_schema_with_diagnostics`] instead.
#[salsa::tracked]
pub fn merged_schema_from_files(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Option<Arc<apollo_compiler::Schema>> {
    merged_schema_with_diagnostics(db, project_files).schema
}

/// Get diagnostics from merging schema files
///
/// This returns validation errors from the schema merge process, such as:
/// - Duplicate type definitions
/// - Interface implementation errors
/// - Union member validation errors
///
/// This is a separate query so callers can get diagnostics without
/// also needing the schema itself.
#[salsa::tracked]
pub fn merged_schema_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<crate::Diagnostic>> {
    merged_schema_with_diagnostics(db, project_files).diagnostics
}

/// Convenience wrapper that extracts `SchemaFiles` from `ProjectFiles`
///
/// **Note**: This function discards validation diagnostics. If you need schema
/// validation errors, use [`merged_schema_with_diagnostics`] instead.
#[salsa::tracked]
pub fn merged_schema(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Option<Arc<apollo_compiler::Schema>> {
    merged_schema_from_files(db, project_files)
}

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;
    use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri};

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
        let mut db = TestDatabase::default();
        let file_id = FileId::new(0);

        let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );
        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);
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
        let mut db = TestDatabase::default();

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

        let schema_files = [
            (file1_id, content1, metadata1),
            (file2_id, content2, metadata2),
        ];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

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
        let mut db = TestDatabase::default();

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

        let schema_files = [
            (file1_id, content1, metadata1),
            (file2_id, content2, metadata2),
        ];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_some(),
            "Expected schema to be merged successfully"
        );

        let schema = schema.unwrap();
        let query_type = schema.types.get("Query");
        assert!(query_type.is_some(), "Expected Query type to exist");

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
        let mut db = TestDatabase::default();

        let project_files = graphql_base_db::test_utils::create_project_files(&mut db, &[], &[]);

        let schema = merged_schema(&db, project_files);
        assert!(schema.is_none(), "Expected None when no schema files exist");
    }

    #[test]
    fn test_merged_schema_invalid_syntax() {
        let mut db = TestDatabase::default();
        let file_id = FileId::new(0);

        let content = FileContent::new(&db, Arc::from("type Query { invalid syntax here"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_none(),
            "Expected None when schema has parse errors"
        );
    }

    #[test]
    fn test_merged_schema_validation_error() {
        let mut db = TestDatabase::default();
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

        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let schema = merged_schema(&db, project_files);
        assert!(
            schema.is_none(),
            "Expected None when schema has validation errors"
        );
    }

    #[test]
    fn test_interface_implementation_missing_field() {
        let mut db = TestDatabase::default();
        let file_id = FileId::new(0);

        // User implements Node but is missing the 'name' field
        let content = FileContent::new(
            &db,
            Arc::from(
                r#"
                interface Node { id: ID! name: String! }
                type User implements Node { id: ID! }
                type Query { user: User }
            "#,
            ),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let result = merged_schema_with_diagnostics(&db, project_files);

        // Schema is still returned (for document validation) but with diagnostics
        assert!(
            result.schema.is_some(),
            "Expected schema to be present (with errors) for document validation"
        );
        assert!(
            !result.diagnostics.is_empty(),
            "Expected diagnostics for missing interface field"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.to_lowercase().contains("name")
                    || d.message.to_lowercase().contains("interface")),
            "Expected error about missing 'name' field. Got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn test_valid_interface_implementation() {
        let mut db = TestDatabase::default();
        let file_id = FileId::new(0);

        // User correctly implements Node
        let content = FileContent::new(
            &db,
            Arc::from(
                r#"
                interface Node { id: ID! }
                type User implements Node { id: ID! name: String! }
                type Query { user: User }
            "#,
            ),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let result = merged_schema_with_diagnostics(&db, project_files);

        assert!(
            result.schema.is_some(),
            "Expected valid schema for correct interface implementation"
        );
        assert!(
            result.diagnostics.is_empty(),
            "Expected no diagnostics. Got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn test_union_non_object_member() {
        let mut db = TestDatabase::default();
        let file_id = FileId::new(0);

        // Union with non-object member (scalar and interface are invalid)
        let content = FileContent::new(
            &db,
            Arc::from(
                r#"
                scalar DateTime
                interface Node { id: ID! }
                union SearchResult = DateTime | Node
                type Query { search: SearchResult }
            "#,
            ),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let result = merged_schema_with_diagnostics(&db, project_files);

        // apollo-compiler should catch union member validation
        // If it doesn't, this test documents the limitation
        assert!(
            !result.diagnostics.is_empty(),
            "Expected diagnostics for non-object union members (scalar DateTime and interface Node). \
            If this assertion fails, apollo-compiler may not validate union member types."
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.to_lowercase().contains("union")
                    || d.message.to_lowercase().contains("object")
                    || d.message.to_lowercase().contains("datetime")
                    || d.message.to_lowercase().contains("node")),
            "Expected error about non-object union members. Got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn test_valid_union() {
        let mut db = TestDatabase::default();
        let file_id = FileId::new(0);

        // Valid union with object type members
        let content = FileContent::new(
            &db,
            Arc::from(
                r#"
                type User { id: ID! name: String! }
                type Post { id: ID! title: String! }
                union SearchResult = User | Post
                type Query { search: SearchResult }
            "#,
            ),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_files, &[]);

        let result = merged_schema_with_diagnostics(&db, project_files);

        assert!(
            result.schema.is_some(),
            "Expected valid schema for correct union definition"
        );
        assert!(
            result.diagnostics.is_empty(),
            "Expected no diagnostics. Got: {:?}",
            result.diagnostics
        );
    }
}
