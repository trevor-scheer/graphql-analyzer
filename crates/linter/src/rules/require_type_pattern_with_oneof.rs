use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that enforces types with `@oneOf` follow a result pattern
///
/// Types annotated with the `@oneOf` directive must contain both `ok` and
/// `error` fields. This pattern standardizes mutation result types by
/// requiring both success and failure case representation.
pub struct RequireTypePatternWithOneofRuleImpl;

impl LintRule for RequireTypePatternWithOneofRuleImpl {
    fn name(&self) -> &'static str {
        "requireTypePatternWithOneof"
    }

    fn description(&self) -> &'static str {
        "Enforces that types with @oneOf directive contain both 'ok' and 'error' fields"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RequireTypePatternWithOneofRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            if type_def.kind != graphql_hir::TypeDefKind::Object {
                continue;
            }

            let has_oneof = type_def
                .directives
                .iter()
                .any(|d| d.name.as_ref() == "oneOf");

            if !has_oneof {
                continue;
            }

            let field_names: Vec<&str> = type_def.fields.iter().map(|f| f.name.as_ref()).collect();

            for field_name in ["error", "ok"] {
                if field_names.contains(&field_name) {
                    continue;
                }

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
                            "type \"{}\" is defined as output with \"@oneOf\" and must be defined with \"{}\" field",
                            type_def.name, field_name
                        ),
                        "requireTypePatternWithOneof",
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
    fn test_oneof_with_ok_and_error_is_valid() {
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        let schema = r"
            type DoSomethingResult @oneOf {
                ok: DoSomethingSuccess
                error: Error
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_oneof_missing_error_field() {
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        let schema = r"
            type DoSomethingResult @oneOf {
                ok: DoSomethingSuccess
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("\"error\""));
    }

    #[test]
    fn test_oneof_missing_ok_field() {
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        let schema = r"
            type DoSomethingResult @oneOf {
                error: Error
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("\"ok\""));
    }

    #[test]
    fn test_oneof_missing_both_fields() {
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        let schema = r"
            type DoSomethingResult @oneOf {
                success: DoSomethingSuccess
                failure: Error
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
        let messages: Vec<_> = all.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("\"ok\"")));
        assert!(messages.iter().any(|m| m.contains("\"error\"")));
    }

    #[test]
    fn test_type_without_oneof_is_ignored() {
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        let schema = r"
            type DoSomethingResult {
                success: Boolean
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_non_object_types_with_oneof_are_ignored() {
        // graphql-eslint's selector only targets ObjectTypeDefinition, so
        // input/interface types annotated with @oneOf must not be flagged.
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        let schema = r"
            input SearchInput @oneOf {
                title: String
                author: String
            }
            interface SearchInterface @oneOf {
                title: String
                author: String
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_empty_type_with_oneof() {
        let db = RootDatabase::default();
        let rule = RequireTypePatternWithOneofRuleImpl;
        // A type with @oneOf but no fields at all
        let schema = "type EmptyResult @oneOf";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
        let messages: Vec<_> = all.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("\"ok\"")));
        assert!(messages.iter().any(|m| m.contains("\"error\"")));
    }
}
