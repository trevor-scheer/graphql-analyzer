use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that requires all fields in `@oneOf` input types to be nullable.
///
/// The `@oneOf` directive indicates that exactly one field must be provided,
/// meaning all fields must be optional (nullable). Non-null fields would
/// prevent valid `@oneOf` usage.
pub struct RequireNullableFieldsWithOneofRuleImpl;

impl LintRule for RequireNullableFieldsWithOneofRuleImpl {
    fn name(&self) -> &'static str {
        "requireNullableFieldsWithOneof"
    }

    fn description(&self) -> &'static str {
        "Requires all fields in @oneOf input types to be nullable"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl StandaloneSchemaLintRule for RequireNullableFieldsWithOneofRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::InputObject {
                continue;
            }

            let has_oneof = type_def
                .directives
                .iter()
                .any(|d| d.name.as_ref() == "oneOf");

            if !has_oneof {
                continue;
            }

            for field in &type_def.fields {
                if field.type_ref.is_non_null {
                    let start: usize = field.name_range.start().into();
                    let end: usize = field.name_range.end().into();
                    let span = graphql_syntax::SourceSpan {
                        start,
                        end,
                        line_offset: 0,
                        byte_offset: 0,
                        source: None,
                    };

                    diagnostics_by_file
                        .entry(type_def.file_id)
                        .or_default()
                        .push(
                            LintDiagnostic::new(
                                span,
                                LintSeverity::Error,
                                format!(
                                    "Field '{}' on @oneOf input type '{}' must be nullable. \
                                     @oneOf requires exactly one field to be provided, so all fields must be optional.",
                                    field.name, type_def.name
                                ),
                                "requireNullableFieldsWithOneof",
                            )
                            .with_help(format!(
                                "Remove the '!' from the type of field '{}' to make it nullable",
                                field.name
                            ))
                            .with_url("https://the-guild.dev/graphql/eslint/rules/require-nullable-fields-with-oneof"),
                        );
                }
            }
        }

        diagnostics_by_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneSchemaLintRule;
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, ProjectFiles, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_schema_project(db: &RootDatabase, schema: &str) -> ProjectFiles {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(schema));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let entry = FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![file_id]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        ProjectFiles::new(
            db,
            schema_file_ids,
            document_file_ids,
            graphql_base_db::ResolvedSchemaFileIds::new(db, std::sync::Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        )
    }

    #[test]
    fn test_oneof_with_nullable_fields() {
        let db = RootDatabase::default();
        let rule = RequireNullableFieldsWithOneofRuleImpl;
        let schema = r#"
input UserByInput @oneOf {
    id: ID
    email: String
    username: String
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_oneof_with_non_null_fields() {
        let db = RootDatabase::default();
        let rule = RequireNullableFieldsWithOneofRuleImpl;
        let schema = r#"
input UserByInput @oneOf {
    id: ID!
    email: String!
    username: String
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
        assert!(all[0].message.contains("must be nullable"));
    }

    #[test]
    fn test_input_without_oneof_allows_non_null() {
        let db = RootDatabase::default();
        let rule = RequireNullableFieldsWithOneofRuleImpl;
        let schema = r#"
input UserInput {
    id: ID!
    email: String!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_non_input_types_ignored() {
        let db = RootDatabase::default();
        let rule = RequireNullableFieldsWithOneofRuleImpl;
        let schema = r#"
type User {
    id: ID!
    name: String!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_oneof_all_non_null() {
        let db = RootDatabase::default();
        let rule = RequireNullableFieldsWithOneofRuleImpl;
        let schema = r#"
input FindUserInput @oneOf {
    id: ID!
    email: String!
    username: String!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 3);
    }
}
