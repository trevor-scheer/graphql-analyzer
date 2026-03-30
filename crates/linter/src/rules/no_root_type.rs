use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `noRootType` rule
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NoRootTypeOptions {
    /// Which root operation types to disallow. Valid values: "Query", "Mutation", "Subscription".
    pub disallow: Vec<String>,
}

impl NoRootTypeOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that disallows specific root operation types in the schema
///
/// Some teams enforce schema governance policies that restrict which root types
/// are allowed. For example, a schema may disallow `Subscription` or `Mutation`
/// to enforce a read-only API pattern.
pub struct NoRootTypeRuleImpl;

impl LintRule for NoRootTypeRuleImpl {
    fn name(&self) -> &'static str {
        "noRootType"
    }

    fn description(&self) -> &'static str {
        "Disallows specific root operation types (Query, Mutation, Subscription) for schema governance"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl StandaloneSchemaLintRule for NoRootTypeRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = NoRootTypeOptions::from_json(options);

        if opts.disallow.is_empty() {
            return HashMap::new();
        }

        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);
        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        // Map each disallowed operation type to its actual type name in the schema
        for disallowed in &opts.disallow {
            let actual_type_name = match disallowed.as_str() {
                "Query" => root_type_names.query.as_deref(),
                "Mutation" => root_type_names.mutation.as_deref(),
                "Subscription" => root_type_names.subscription.as_deref(),
                _ => continue,
            };

            let Some(type_name) = actual_type_name else {
                continue;
            };

            let Some(type_def) = schema_types.get(type_name) else {
                continue;
            };

            let start: usize = type_def.name_range.start().into();
            let end: usize = type_def.name_range.end().into();
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
                .push(LintDiagnostic::new(
                    span,
                    LintSeverity::Error,
                    format!(
                        "{disallowed} root type '{}' is not allowed by schema governance policy",
                        type_def.name
                    ),
                    "noRootType",
                ));
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

    fn make_options(disallow: &[&str]) -> serde_json::Value {
        serde_json::json!({ "disallow": disallow })
    }

    #[test]
    fn test_disallow_subscription() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = r#"
            type Query { hello: String }
            type Mutation { doSomething: Boolean }
            type Subscription { onEvent: String }
        "#;
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["Subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Subscription"));
        assert!(all[0].message.contains("not allowed"));
    }

    #[test]
    fn test_disallow_mutation() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = r#"
            type Query { hello: String }
            type Mutation { doSomething: Boolean }
        "#;
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["Mutation"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Mutation"));
    }

    #[test]
    fn test_allows_types_not_in_disallow_list() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = r#"
            type Query { hello: String }
            type Mutation { doSomething: Boolean }
            type Subscription { onEvent: String }
        "#;
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["Subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        // Only Subscription should be flagged, not Query or Mutation
        assert_eq!(all.len(), 1);
        assert!(!all[0].message.contains("Query"));
        assert!(!all[0].message.contains("Mutation"));
    }

    #[test]
    fn test_empty_disallow_list_produces_no_diagnostics() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = r#"
            type Query { hello: String }
            type Mutation { doSomething: Boolean }
            type Subscription { onEvent: String }
        "#;
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&[]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_no_options_produces_no_diagnostics() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = r#"
            type Query { hello: String }
            type Mutation { doSomething: Boolean }
        "#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_disallow_nonexistent_type() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { hello: String }";
        let project_files = create_schema_project(&db, schema);
        // Disallow Subscription, but it doesn't exist in the schema
        let opts = make_options(&["Subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_disallow_multiple_root_types() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = r#"
            type Query { hello: String }
            type Mutation { doSomething: Boolean }
            type Subscription { onEvent: String }
        "#;
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["Mutation", "Subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_rule_metadata() {
        let rule = NoRootTypeRuleImpl;
        assert_eq!(rule.name(), "noRootType");
        assert_eq!(rule.default_severity(), LintSeverity::Error);
        assert!(!rule.description().is_empty());
    }
}
