use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that requires a reason in @deprecated directives
pub struct RequireDeprecationReasonRuleImpl;

impl LintRule for RequireDeprecationReasonRuleImpl {
    fn name(&self) -> &'static str {
        "require_deprecation_reason"
    }

    fn description(&self) -> &'static str {
        "Requires a reason argument in @deprecated directives"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RequireDeprecationReasonRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            // Check fields
            for field in &type_def.fields {
                if field.is_deprecated && field.deprecation_reason.is_none() {
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
                        .push(LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            format!(
                                "Field '{}.{}' is deprecated without a reason",
                                type_def.name, field.name
                            ),
                            "require_deprecation_reason",
                        ));
                }

                // Check arguments
                for arg in &field.arguments {
                    if arg.is_deprecated && arg.deprecation_reason.is_none() {
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
                            .push(LintDiagnostic::new(
                                span,
                                LintSeverity::Warning,
                                format!(
                                    "Argument '{}' on '{}.{}' is deprecated without a reason",
                                    arg.name, type_def.name, field.name
                                ),
                                "require_deprecation_reason",
                            ));
                    }
                }
            }

            // Check enum values
            for ev in &type_def.enum_values {
                if ev.is_deprecated && ev.deprecation_reason.is_none() {
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
                                "Enum value '{}.{}' is deprecated without a reason",
                                type_def.name, ev.name
                            ),
                            "require_deprecation_reason",
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
    fn test_deprecated_with_reason() {
        let db = RootDatabase::default();
        let rule = RequireDeprecationReasonRuleImpl;
        let schema = r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField")
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_deprecated_without_reason() {
        let db = RootDatabase::default();
        let rule = RequireDeprecationReasonRuleImpl;
        let schema = r"
type User {
    id: ID!
    oldField: String @deprecated
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("without a reason"));
    }
}
