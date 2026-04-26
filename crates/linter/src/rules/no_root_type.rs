use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `noRootType` rule
#[derive(Debug, Clone, Deserialize)]
pub struct NoRootTypeOptions {
    /// Which root types to disallow. Valid values: "query", "mutation", "subscription".
    pub disallow: Vec<RootTypeKind>,
}

/// A root operation type that can be disallowed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RootTypeKind {
    Query,
    Mutation,
    Subscription,
}

impl std::fmt::Display for RootTypeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Query => write!(f, "query"),
            Self::Mutation => write!(f, "mutation"),
            Self::Subscription => write!(f, "subscription"),
        }
    }
}

impl NoRootTypeOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Option<Self> {
        value.and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Lint rule that disallows certain root type definitions in the schema
///
/// Some projects may want to forbid specific root types. For example, a
/// read-only API might disallow `Mutation`, or a project that doesn't use
/// subscriptions might disallow `Subscription`.
pub struct NoRootTypeRuleImpl;

impl LintRule for NoRootTypeRuleImpl {
    fn name(&self) -> &'static str {
        "noRootType"
    }

    fn description(&self) -> &'static str {
        "Disallows certain root type definitions in the schema"
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
        let Some(opts) = NoRootTypeOptions::from_json(options) else {
            // Without options, we don't know which root types to disallow
            return HashMap::new();
        };

        if opts.disallow.is_empty() {
            return HashMap::new();
        }

        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);
        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        for disallowed in &opts.disallow {
            let root_type_name = match disallowed {
                RootTypeKind::Query => root_type_names.query.as_deref(),
                RootTypeKind::Mutation => root_type_names.mutation.as_deref(),
                RootTypeKind::Subscription => root_type_names.subscription.as_deref(),
            };

            let Some(type_name) = root_type_name else {
                continue;
            };

            let Some(type_def) = schema_types.get(type_name) else {
                continue;
            };

            // Point at the type name (matches graphql-eslint's
            // `node.name.loc`). Falls back to the definition span if the name
            // range is empty (defensive).
            let (start, end): (usize, usize) =
                if type_def.name_range.start() == type_def.name_range.end() {
                    (
                        type_def.definition_range.start().into(),
                        type_def.definition_range.end().into(),
                    )
                } else {
                    (
                        type_def.name_range.start().into(),
                        type_def.name_range.end().into(),
                    )
                };
            let span = graphql_syntax::SourceSpan {
                start,
                end,
                line_offset: 0,
                byte_offset: 0,
                source: None,
            };

            // Suggestion: remove the entire root type def (matches
            // upstream's `fixer.remove(node.parent)`).
            let def_start: usize = type_def.definition_range.start().into();
            let def_end: usize = type_def.definition_range.end().into();
            let suggestion =
                CodeSuggestion::delete(format!("Remove `{type_name}` type"), def_start, def_end);

            diagnostics_by_file
                .entry(type_def.file_id)
                .or_default()
                .push(
                    LintDiagnostic::new(
                        span,
                        LintSeverity::Error,
                        format!("Root type `{type_name}` is forbidden."),
                        "noRootType",
                    )
                    .with_suggestion(suggestion)
                    .with_help(format!("Remove `{type_name}` type")),
                );
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

    fn make_options(disallow: &[&str]) -> serde_json::Value {
        serde_json::json!({ "disallow": disallow })
    }

    #[test]
    fn test_no_options_produces_no_diagnostics() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String } type Mutation { doThing: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_empty_disallow_produces_no_diagnostics() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String } type Mutation { doThing: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&[]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_disallow_mutation() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String } type Mutation { doThing: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["mutation"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Mutation"));
        assert!(all[0].message.contains("forbidden"));
    }

    #[test]
    fn test_disallow_subscription() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String } type Subscription { onEvent: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Subscription"));
    }

    #[test]
    fn test_disallow_multiple_root_types() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String } type Mutation { doThing: String } type Subscription { onEvent: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["mutation", "subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_allowed_root_type_not_flagged() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String } type Mutation { doThing: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["subscription"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_disallow_query() {
        let db = RootDatabase::default();
        let rule = NoRootTypeRuleImpl;
        let schema = "type Query { field: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = make_options(&["query"]);
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Query"));
    }
}
