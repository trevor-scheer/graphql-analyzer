use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that warns when root type fields have non-nullable return types
///
/// Root fields (on Query, Mutation, Subscription) with non-nullable return types
/// prevent partial failure resilience. If one root field resolver fails, a non-null
/// return type causes the entire response to be nullified up the tree. Making root
/// fields nullable allows other fields to still return data on partial failures.
pub struct RequireNullableResultInRootRuleImpl;

impl LintRule for RequireNullableResultInRootRuleImpl {
    fn name(&self) -> &'static str {
        "requireNullableResultInRoot"
    }

    fn description(&self) -> &'static str {
        "Warns when root type fields have non-nullable return types"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// Format a `TypeRef` as a GraphQL type string (e.g. `User!`, `[User!]!`, `[User!]`)
fn format_type_ref(type_ref: &graphql_hir::TypeRef) -> String {
    let name = type_ref.name.as_ref();
    match (
        type_ref.is_list,
        type_ref.is_non_null,
        type_ref.inner_non_null,
    ) {
        (false, false, _) => name.to_string(),
        (false, true, _) => format!("{name}!"),
        (true, false, false) => format!("[{name}]"),
        (true, false, true) => format!("[{name}!]"),
        (true, true, false) => format!("[{name}]!"),
        (true, true, true) => format!("[{name}!]!"),
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

        // Collect all root type names to check
        let root_names: Vec<&str> = [
            root_type_names.query.as_deref(),
            root_type_names.mutation.as_deref(),
            root_type_names.subscription.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();

        for root_name in root_names {
            let Some(root_type) = schema_types.get(root_name) else {
                continue;
            };

            for field in &root_type.fields {
                if field.type_ref.is_non_null {
                    let type_string = format_type_ref(&field.type_ref);

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
                        .entry(root_type.file_id)
                        .or_default()
                        .push(LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            format!(
                                "Root field '{}' has non-nullable return type '{}' \
                                 — consider making it nullable for partial failure resilience",
                                field.name, type_string
                            ),
                            "requireNullableResultInRoot",
                        ));
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
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_non_nullable_query_field_warns() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { user(id: ID!): User! } type User { id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("user"));
        assert!(all[0].message.contains("User!"));
    }

    #[test]
    fn test_nullable_query_field_no_warning() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { user(id: ID!): User } type User { id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_non_nullable_list_warns() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { users: [User!]! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("users"));
        assert!(all[0].message.contains("users"));
        assert!(all[0].message.contains("[User"));
    }

    #[test]
    fn test_nullable_list_no_warning() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { users: [User!] } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_nested_type_fields_not_checked() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema = "type Query { user: User } type User { id: ID! name: String! email: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_mutation_non_nullable_warns() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema =
            "type Query { ok: Boolean } type Mutation { createUser: User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("createUser"));
    }

    #[test]
    fn test_subscription_non_nullable_warns() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema =
            "type Query { ok: Boolean } type Subscription { onUser: User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("onUser"));
    }

    #[test]
    fn test_custom_root_type_names() {
        let db = RootDatabase::default();
        let rule = RequireNullableResultInRootRuleImpl;
        let schema =
            "schema { query: RootQuery } type RootQuery { user: User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("user"));
    }
}
