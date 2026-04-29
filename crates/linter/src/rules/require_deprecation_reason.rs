use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

fn type_def_kind_display(kind: TypeDefKind) -> &'static str {
    match kind {
        TypeDefKind::Interface => "interface",
        TypeDefKind::Union => "union",
        TypeDefKind::Enum => "enum",
        TypeDefKind::Scalar => "scalar",
        TypeDefKind::InputObject => "input",
        // `Object` and any future TypeDefKind variants fall through to the
        // generic "type" — matches graphql-eslint's `displayNodeName` default.
        _ => "type",
    }
}

/// Lint rule that requires a reason in @deprecated directives
pub struct RequireDeprecationReasonRuleImpl;

impl LintRule for RequireDeprecationReasonRuleImpl {
    fn name(&self) -> &'static str {
        "requireDeprecationReason"
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
        // Helper: locate the @deprecated directive's name_range on a slice of
        // directives. graphql-eslint reports diagnostics on the directive's
        // name node; we mirror that to keep span parity.
        fn deprecated_name_range(
            directives: &[graphql_hir::DirectiveUsage],
        ) -> Option<graphql_hir::TextRange> {
            directives
                .iter()
                .find(|d| &*d.name == "deprecated")
                .map(|d| d.name_range)
        }

        // Mirrors upstream's `value && String(valueFromNode(...)).trim()` check:
        // absent, empty string, and whitespace-only are all treated as missing.
        fn is_missing_reason(reason: Option<&str>) -> bool {
            reason.map_or(true, |r| r.trim().is_empty())
        }

        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            // Check type-level @deprecated (e.g. `type MyQuery @deprecated`)
            let type_deprecated = type_def
                .directives
                .iter()
                .find(|d| &*d.name == "deprecated");
            if let Some(dep_dir) = type_deprecated {
                // DirectiveUsage stores argument values via `ast::Value::to_string()`,
                // which includes surrounding quotes for string literals. Strip them
                // before applying the empty/whitespace check.
                let reason_str = dep_dir.arguments.iter().find_map(|a| {
                    if &*a.name == "reason" {
                        Some(a.value.trim_matches('"'))
                    } else {
                        None
                    }
                });
                if is_missing_reason(reason_str) {
                    let start: usize = dep_dir.name_range.start().into();
                    let end: usize = dep_dir.name_range.end().into();
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
                                LintSeverity::Warning,
                                format!(
                                    "Deprecation reason is required for {} \"{}\".",
                                    type_def_kind_display(type_def.kind),
                                    type_def.name
                                ),
                                "requireDeprecationReason",
                            )
                            .with_help(
                                "Add a reason string to @deprecated explaining why and what to use instead",
                            ),
                        );
                }
            }

            // Check fields (including input object fields)
            for field in &type_def.fields {
                if field.is_deprecated && is_missing_reason(field.deprecation_reason.as_deref()) {
                    let range =
                        deprecated_name_range(&field.directives).unwrap_or(field.name_range);
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
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
                                LintSeverity::Warning,
                                format!(
                                    "Deprecation reason is required for field \"{}\" in {} \"{}\".",
                                    field.name,
                                    type_def_kind_display(type_def.kind),
                                    type_def.name
                                ),
                                "requireDeprecationReason",
                            )
                            .with_help(
                                "Add a reason string to @deprecated explaining why and what to use instead",
                            ),
                        );
                }

                // Check arguments
                for arg in &field.arguments {
                    if arg.is_deprecated && is_missing_reason(arg.deprecation_reason.as_deref()) {
                        let range =
                            deprecated_name_range(&arg.directives).unwrap_or(field.name_range);
                        let start: usize = range.start().into();
                        let end: usize = range.end().into();
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
                                    LintSeverity::Warning,
                                    format!(
                                        "Deprecation reason is required for input value \"{}\" in field \"{}\".",
                                        arg.name, field.name
                                    ),
                                    "requireDeprecationReason",
                                )
                                .with_help(
                                    "Add a reason string to @deprecated explaining why and what to use instead",
                                ),
                            );
                    }
                }
            }

            // Check enum values
            for ev in &type_def.enum_values {
                if ev.is_deprecated && is_missing_reason(ev.deprecation_reason.as_deref()) {
                    let range =
                        deprecated_name_range(&ev.directives).unwrap_or(type_def.name_range);
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
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
                                LintSeverity::Warning,
                                format!(
                                    "Deprecation reason is required for enum value \"{}\" in enum \"{}\".",
                                    ev.name, type_def.name
                                ),
                                "requireDeprecationReason",
                            )
                            .with_help(
                                "Add a reason string to @deprecated explaining why and what to use instead",
                            ),
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
        assert!(all[0].message.contains("Deprecation reason is required"));
    }

    #[test]
    fn test_deprecated_with_empty_reason() {
        let db = RootDatabase::default();
        let rule = RequireDeprecationReasonRuleImpl;
        let schema = r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "")
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Deprecation reason is required"));
    }

    #[test]
    fn test_type_level_deprecated_without_reason() {
        let db = RootDatabase::default();
        let rule = RequireDeprecationReasonRuleImpl;
        let schema = r"
type MyQuery @deprecated {
    field: String
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Deprecation reason is required"));
        assert!(all[0].message.contains("MyQuery"));
    }
}
