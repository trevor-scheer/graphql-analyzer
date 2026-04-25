use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `requireDeprecationDate` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RequireDeprecationDateOptions {
    /// The argument name to search for in the deprecation reason string.
    /// Defaults to `"deletionDate"`.
    #[serde(rename = "argumentName")]
    pub argument_name: String,
}

impl Default for RequireDeprecationDateOptions {
    fn default() -> Self {
        Self {
            argument_name: "deletionDate".to_string(),
        }
    }
}

impl RequireDeprecationDateOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that requires `@deprecated` directives to include a deletion/removal date
/// in the deprecation reason string.
///
/// This helps teams track when deprecated fields should be removed by requiring
/// a date to be included in the deprecation reason via a pattern like
/// `deletionDate: 01/01/2025`.
// TODO(parity): graphql-eslint reads the deletion date from a dedicated
// `deletionDate` argument on the `@deprecated` directive (not the reason
// string), and additionally reports MESSAGE_INVALID_FORMAT (`Deletion date
// must be in format "DD/MM/YYYY" for ...`), MESSAGE_INVALID_DATE (`Invalid
// "..." deletion date for ...`), and MESSAGE_CAN_BE_REMOVED (`... can be
// removed`). Our implementation only checks for the argument-name substring
// inside the reason string and only emits the "must have a deletion date"
// message.
pub struct RequireDeprecationDateRuleImpl;

impl LintRule for RequireDeprecationDateRuleImpl {
    fn name(&self) -> &'static str {
        "requireDeprecationDate"
    }

    fn description(&self) -> &'static str {
        "Requires @deprecated directives to include a deletion date in the reason"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// Check if the deprecation reason contains a date argument pattern.
///
/// Looks for patterns like `deletionDate: 01/01/2025` or `deletionDate: 2025-01-01`
/// within the reason string.
fn reason_contains_date_argument(reason: &str, argument_name: &str) -> bool {
    // Look for the argument name followed by a colon and a date-like value
    // e.g. "deletionDate: 01/01/2025" or "deletionDate: 2025-01-01"
    let Some(pos) = reason.find(argument_name) else {
        return false;
    };

    let after = &reason[pos + argument_name.len()..];
    // Expect optional whitespace, then a colon or equals
    let trimmed = after.trim_start();
    if trimmed.starts_with(':') || trimmed.starts_with('=') {
        let value = trimmed[1..].trim_start();
        // Check that there's some non-empty value after the separator
        !value.is_empty()
    } else {
        false
    }
}

impl StandaloneSchemaLintRule for RequireDeprecationDateRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = RequireDeprecationDateOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            // Check fields
            for field in &type_def.fields {
                if field.is_deprecated {
                    let has_date = field.deprecation_reason.as_ref().is_some_and(|reason| {
                        reason_contains_date_argument(reason, &opts.argument_name)
                    });

                    if !has_date {
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
                                    LintSeverity::Warning,
                                    format!(
                                        "Directive `@deprecated` must have a deletion date for field `{}` in type `{}`",
                                        field.name, type_def.name
                                    ),
                                    "requireDeprecationDate",
                                ),
                            );
                    }
                }

                // Check arguments
                for arg in &field.arguments {
                    if arg.is_deprecated {
                        let has_date = arg.deprecation_reason.as_ref().is_some_and(|reason| {
                            reason_contains_date_argument(reason, &opts.argument_name)
                        });

                        if !has_date {
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
                                        LintSeverity::Warning,
                                        format!(
                                            "Directive `@deprecated` must have a deletion date for input value `{}` in field `{}`",
                                            arg.name, field.name
                                        ),
                                        "requireDeprecationDate",
                                    ),
                                );
                        }
                    }
                }
            }

            // Check enum values
            for ev in &type_def.enum_values {
                if ev.is_deprecated {
                    let has_date = ev.deprecation_reason.as_ref().is_some_and(|reason| {
                        reason_contains_date_argument(reason, &opts.argument_name)
                    });

                    if !has_date {
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
                            .push(
                                LintDiagnostic::new(
                                    span,
                                    LintSeverity::Warning,
                                    format!(
                                        "Directive `@deprecated` must have a deletion date for enum value `{}` in enum `{}`",
                                        ev.name, type_def.name
                                    ),
                                    "requireDeprecationDate",
                                ),
                            );
                    }
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

    fn check_with_options(
        schema: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = RequireDeprecationDateRuleImpl;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, options);
        diagnostics.into_values().flatten().collect()
    }

    fn check(schema: &str) -> Vec<LintDiagnostic> {
        check_with_options(schema, None)
    }

    #[test]
    fn test_deprecated_with_deletion_date() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField instead, deletionDate: 01/01/2025")
}
"#,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_deprecated_without_deletion_date() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField instead")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("must have a deletion date"));
    }

    #[test]
    fn test_deprecated_without_reason() {
        let diagnostics = check(
            r"
type User {
    id: ID!
    oldField: String @deprecated
}
",
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("must have a deletion date"));
    }

    #[test]
    fn test_enum_deprecated_without_date() {
        let diagnostics = check(
            r#"
enum Status {
    ACTIVE
    LEGACY @deprecated(reason: "No longer used")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("enum value"));
    }

    #[test]
    fn test_enum_deprecated_with_date() {
        let diagnostics = check(
            r#"
enum Status {
    ACTIVE
    LEGACY @deprecated(reason: "No longer used, deletionDate: 2025-06-01")
}
"#,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_no_deprecated_fields() {
        let diagnostics = check(
            r"
type User {
    id: ID!
    name: String
}
",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_custom_argument_name() {
        let opts = serde_json::json!({ "argumentName": "removalDate" });
        let diagnostics = check_with_options(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField, removalDate: 2025-03-01")
}
"#,
            Some(&opts),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_custom_argument_name_missing() {
        let opts = serde_json::json!({ "argumentName": "removalDate" });
        let diagnostics = check_with_options(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField, deletionDate: 2025-03-01")
}
"#,
            Some(&opts),
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_multiple_deprecated_fields() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField1: String @deprecated(reason: "deletionDate: 2025-01-01")
    oldField2: String @deprecated(reason: "Will be removed")
    oldField3: String @deprecated
}
"#,
        );
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_deletion_date_with_equals_separator() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField, deletionDate = 2025-01-01")
}
"#,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reason_contains_date_argument_patterns() {
        assert!(reason_contains_date_argument(
            "deletionDate: 01/01/2025",
            "deletionDate"
        ));
        assert!(reason_contains_date_argument(
            "Use newField, deletionDate: 2025-01-01",
            "deletionDate"
        ));
        assert!(reason_contains_date_argument(
            "deletionDate = 2025-01-01",
            "deletionDate"
        ));
        assert!(reason_contains_date_argument(
            "deletionDate:2025-01-01",
            "deletionDate"
        ));
        assert!(!reason_contains_date_argument(
            "Use newField instead",
            "deletionDate"
        ));
        assert!(!reason_contains_date_argument(
            "deletionDate",
            "deletionDate"
        ));
        assert!(!reason_contains_date_argument(
            "deletionDate: ",
            "deletionDate"
        ));
    }
}
