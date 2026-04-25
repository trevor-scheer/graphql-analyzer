use crate::diagnostics::{rule_doc_url, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that requires root type fields to return nullable types
///
/// Root type fields (Query, Mutation, Subscription) should return nullable types
/// to improve resilience. If a root field returns a non-null type and an error
/// occurs, GraphQL's null-bubbling behavior propagates the null up to the nearest
/// nullable parent — potentially nulling out the entire `data` response.
///
/// By making root fields nullable, errors are isolated to the individual field
/// that failed, and the rest of the response remains intact.
pub struct RequireNullableResultInRootRuleImpl;

impl LintRule for RequireNullableResultInRootRuleImpl {
    fn name(&self) -> &'static str {
        "requireNullableResultInRoot"
    }

    fn description(&self) -> &'static str {
        "Requires root type fields to return nullable types for error resilience"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RequireNullableResultInRootRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        // Check each root type (Query, Mutation, Subscription)
        let root_entries = [
            &root_type_names.query,
            &root_type_names.mutation,
            &root_type_names.subscription,
        ];

        for root_name in root_entries {
            let Some(type_name) = root_name else {
                continue;
            };

            let Some(root_type) = schema_types.get(type_name.as_str()) else {
                continue;
            };

            for field in &root_type.fields {
                if field.type_ref.is_non_null {
                    let span = graphql_syntax::SourceSpan {
                        start: field.type_ref.name_range.start().into(),
                        end: field.type_ref.name_range.end().into(),
                        line_offset: 0,
                        byte_offset: 0,
                        source: None,
                    };

                    diagnostics_by_file.entry(field.file_id).or_default().push(
                        LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            format!(
                                "Unexpected non-null result {} in {}",
                                field.type_ref.name, type_name
                            ),
                            "requireNullableResultInRoot",
                        )
                        .with_url(rule_doc_url("requireNullableResultInRoot")),
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
    fn test_nullable_root_fields_pass() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { user(id: ID!): User } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_non_null_query_field_fails() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { user(id: ID!): User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].message, "Unexpected non-null result User in Query");
        // Diagnostic spans the inner named type (`User`), matching the
        // `TypeRef.name_range` provenance from the HIR.
        let span = &all[0].span;
        assert_eq!(&schema[span.start..span.end], "User");
    }

    #[test]
    fn test_non_null_mutation_field_fails() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { _: String } type Mutation { createUser(name: String!): User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected non-null result User in Mutation"
        );
    }

    #[test]
    fn test_non_null_subscription_field_fails() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { _: String } type Subscription { onMessage: Message! } type Message { text: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected non-null result Message in Subscription"
        );
    }

    #[test]
    fn test_non_null_list_fails() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { users: [User!]! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("non-null result"));
        assert!(all[0].message.contains("in Query"));
        // Diagnostic spans the inner named type (`User`) inside the wrapper.
        let span = &all[0].span;
        assert_eq!(&schema[span.start..span.end], "User");
    }

    #[test]
    fn test_nullable_list_passes() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { users: [User!] } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_multiple_non_null_fields() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema =
            "type Query { user: User!, post: Post! } type User { id: ID! } type Post { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_no_root_types() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_custom_root_type_names() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = r"
            schema { query: RootQuery }
            type RootQuery { user: User! }
            type User { id: ID! }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected non-null result User in RootQuery"
        );
    }

    #[test]
    fn test_non_root_type_fields_ignored() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        // Non-null fields on non-root types should not trigger
        let schema = "type Query { user: User } type User { id: ID!, name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_mixed_nullable_and_non_null() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema =
            "type Query { user: User, post: Post! } type User { id: ID! } type Post { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].message, "Unexpected non-null result Post in Query");
    }
}
