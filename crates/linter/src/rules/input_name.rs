use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `input_name` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InputNameOptions {
    /// Required suffix for input type names. Defaults to "Input".
    pub suffix: String,
}

impl Default for InputNameOptions {
    fn default() -> Self {
        Self {
            suffix: "Input".to_string(),
        }
    }
}

impl InputNameOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces naming convention for input types
///
/// Input type names should end with "Input" (or a configurable suffix)
/// to distinguish them from output types.
pub struct InputNameRuleImpl;

impl LintRule for InputNameRuleImpl {
    fn name(&self) -> &'static str {
        "input_name"
    }

    fn description(&self) -> &'static str {
        "Enforces that input type names end with a specific suffix"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for InputNameRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = InputNameOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::InputObject {
                continue;
            }

            if !type_def.name.ends_with(&opts.suffix) {
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
                        LintSeverity::Warning,
                        format!(
                            "Input type '{}' should end with '{}'",
                            type_def.name, opts.suffix
                        ),
                        "input_name",
                    ));
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
    fn test_input_with_suffix() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "input CreateUserInput { name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_input_without_suffix() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "input CreateUser { name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("should end with 'Input'"));
    }
}
