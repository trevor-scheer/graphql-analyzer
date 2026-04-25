use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that enforces the Relay specification for the `PageInfo` type.
///
/// The `PageInfo` type must contain the following fields with exact types:
/// - `hasPreviousPage: Boolean!`
/// - `hasNextPage: Boolean!`
/// - `startCursor: String`
/// - `endCursor: String`
pub struct RelayPageInfoRuleImpl;

impl LintRule for RelayPageInfoRuleImpl {
    fn name(&self) -> &'static str {
        "relayPageInfo"
    }

    fn description(&self) -> &'static str {
        "Enforces that the PageInfo type follows the Relay specification"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// A required field on the `PageInfo` type per the Relay spec.
struct RequiredField {
    name: &'static str,
    type_name: &'static str,
    is_non_null: bool,
}

const REQUIRED_FIELDS: &[RequiredField] = &[
    RequiredField {
        name: "hasPreviousPage",
        type_name: "Boolean",
        is_non_null: true,
    },
    RequiredField {
        name: "hasNextPage",
        type_name: "Boolean",
        is_non_null: true,
    },
    RequiredField {
        name: "startCursor",
        type_name: "String",
        is_non_null: false,
    },
    RequiredField {
        name: "endCursor",
        type_name: "String",
        is_non_null: false,
    },
];

impl StandaloneSchemaLintRule for RelayPageInfoRuleImpl {
    // TODO(parity): graphql-eslint allows the cursor field type to be `String` OR any Scalar
    // (via `isScalarType`). We currently only allow `String`. Custom scalar cursors
    // (e.g. `ID`, custom `Cursor` scalars) should pass without diagnostic.
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // TODO(parity): graphql-eslint reports "The server must provide a `PageInfo` object."
        // on the first character of the schema entry file when PageInfo is absent. We currently
        // skip diagnostics in that case.
        let Some(page_info) = schema_types.get("PageInfo") else {
            return diagnostics_by_file;
        };

        if page_info.kind != TypeDefKind::Object {
            let start: usize = page_info.name_range.start().into();
            let end: usize = page_info.name_range.end().into();
            let span = graphql_syntax::SourceSpan {
                start,
                end,
                line_offset: 0,
                byte_offset: 0,
                source: None,
            };
            diagnostics_by_file
                .entry(page_info.file_id)
                .or_default()
                .push(
                    LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        "`PageInfo` must be an Object type.",
                        "relayPageInfo",
                    )
                    .with_url("https://relay.dev/graphql/connections.htm#sec-undefined.PageInfo"),
                );
            return diagnostics_by_file;
        }

        for required in REQUIRED_FIELDS {
            if let Some(field) = page_info
                .fields
                .iter()
                .find(|f| f.name.as_ref() == required.name)
            {
                // Field exists - check its type
                let type_matches = field.type_ref.name.as_ref() == required.type_name
                    && !field.type_ref.is_list
                    && field.type_ref.is_non_null == required.is_non_null;

                if !type_matches {
                    let expected_type = if required.is_non_null {
                        format!("{}!", required.type_name)
                    } else {
                        required.type_name.to_string()
                    };
                    let return_type = if required.is_non_null {
                        "non-null Boolean".to_string()
                    } else {
                        "either String or Scalar, which can be null if there are no results"
                            .to_string()
                    };

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
                        .entry(page_info.file_id)
                        .or_default()
                        .push(
                            LintDiagnostic::new(
                                span,
                                LintSeverity::Warning,
                                format!("Field `{}` must return {}.", required.name, return_type),
                                "relayPageInfo",
                            )
                            .with_help(format!(
                                "Change the type of `{}` to `{}`",
                                required.name, expected_type
                            ))
                            .with_url(
                                "https://relay.dev/graphql/connections.htm#sec-undefined.PageInfo",
                            ),
                        );
                }
            } else {
                // Field is missing entirely - report on the type name
                let expected_type = if required.is_non_null {
                    format!("{}!", required.type_name)
                } else {
                    required.type_name.to_string()
                };
                let return_type = if required.is_non_null {
                    "non-null Boolean".to_string()
                } else {
                    "either String or Scalar, which can be null if there are no results".to_string()
                };

                let start: usize = page_info.name_range.start().into();
                let end: usize = page_info.name_range.end().into();
                let span = graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };
                diagnostics_by_file
                    .entry(page_info.file_id)
                    .or_default()
                    .push(
                        LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            format!(
                                "`PageInfo` must contain a field `{}`, that return {}.",
                                required.name, return_type
                            ),
                            "relayPageInfo",
                        )
                        .with_help(format!(
                            "Add field `{}: {}` to the `PageInfo` type",
                            required.name, expected_type
                        ))
                        .with_url(
                            "https://relay.dev/graphql/connections.htm#sec-undefined.PageInfo",
                        ),
                    );
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
    fn valid_page_info() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = r"
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: Boolean!
                startCursor: String
                endCursor: String
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn valid_page_info_with_extra_fields() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = r"
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: Boolean!
                startCursor: String
                endCursor: String
                totalCount: Int
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn no_page_info_type() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = "type Query { hello: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn missing_all_fields() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = r"
            type PageInfo {
                totalCount: Int
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 4);
        assert!(all.iter().any(|d| d.message.contains("hasPreviousPage")));
        assert!(all.iter().any(|d| d.message.contains("hasNextPage")));
        assert!(all.iter().any(|d| d.message.contains("startCursor")));
        assert!(all.iter().any(|d| d.message.contains("endCursor")));
    }

    #[test]
    fn wrong_type_for_has_next_page() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = r"
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: String
                startCursor: String
                endCursor: String
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("hasNextPage"));
        assert!(all[0].message.contains("non-null Boolean"));
    }

    #[test]
    fn nullable_boolean_fields() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = r"
            type PageInfo {
                hasPreviousPage: Boolean
                hasNextPage: Boolean
                startCursor: String
                endCursor: String
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|d| d.message.contains("hasPreviousPage")));
        assert!(all.iter().any(|d| d.message.contains("hasNextPage")));
    }

    #[test]
    fn non_null_cursor_fields_is_valid() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        // Cursors as non-null is stricter than required but still valid
        // Actually, the Relay spec says String (nullable), so non-null is wrong type
        let schema = r"
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: Boolean!
                startCursor: String!
                endCursor: String!
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        // Non-null cursors don't match the spec (String, not String!)
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|d| d.message.contains("startCursor")));
        assert!(all.iter().any(|d| d.message.contains("endCursor")));
    }

    #[test]
    fn page_info_not_object_type() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = "scalar PageInfo";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("`PageInfo` must be an Object type"));
    }
}
