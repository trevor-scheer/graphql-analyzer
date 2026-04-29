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
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Collect all raw PageInfo declarations (base + each extension).  Upstream fires
        // once per AST node; we mirror that by iterating declarations rather than the
        // merged type so that `union PageInfo = A` and `extend union PageInfo = B` each
        // produce their own "must be Object type" diagnostic.
        let raw_page_infos: Vec<_> = crate::schema_utils::raw_schema_type_defs(db, project_files)
            .into_iter()
            .filter(|(_, td)| td.name.as_ref() == "PageInfo")
            .collect();

        if raw_page_infos.is_empty() {
            // Mirrors graphql-eslint: when PageInfo is entirely absent, emit a single
            // diagnostic anchored at the first character of the first schema file.
            let schema_ids = project_files.schema_file_ids(db).ids(db);
            if let Some(&file_id) = schema_ids.first() {
                let span = graphql_syntax::SourceSpan {
                    start: 0,
                    end: 1,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };
                diagnostics_by_file.entry(file_id).or_default().push(
                    LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        "The server must provide a `PageInfo` object.",
                        "relayPageInfo",
                    )
                    .with_message_id("MESSAGE_MUST_EXIST")
                    .with_url("https://relay.dev/graphql/connections.htm#sec-undefined.PageInfo"),
                );
            }
            return diagnostics_by_file;
        }

        for (_, page_info) in &raw_page_infos {
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
                        .with_message_id("MESSAGE_MUST_BE_OBJECT_TYPE")
                        .with_url(
                            "https://relay.dev/graphql/connections.htm#sec-undefined.PageInfo",
                        ),
                    );
                continue;
            }

            for required in REQUIRED_FIELDS {
                if let Some(field) = page_info
                    .fields
                    .iter()
                    .find(|f| f.name.as_ref() == required.name)
                {
                    // Field exists - check its type. Boolean fields require an exact
                    // `Boolean!` match. Cursor fields (nullable `String`) match either
                    // `String` or any other scalar in the schema, mirroring graphql-eslint's
                    // `isScalarType` relaxation.
                    // GraphQL built-in scalars aren't always present in
                    // `schema_types`, which is keyed off user-declared types.
                    // Treat them as scalars so e.g. `ID` is accepted as a cursor
                    // type (matches graphql-eslint's `isScalarType` semantics).
                    const BUILTIN_SCALARS: &[&str] = &["String", "Int", "Float", "Boolean", "ID"];
                    let referent_name = field.type_ref.name.as_ref();
                    let referent_is_scalar = BUILTIN_SCALARS.contains(&referent_name)
                        || schema_types
                            .get(referent_name)
                            .is_some_and(|t| t.kind == TypeDefKind::Scalar);

                    let type_matches = if required.is_non_null {
                        referent_name == required.type_name
                            && !field.type_ref.is_list
                            && field.type_ref.is_non_null
                    } else {
                        !field.type_ref.is_list
                            && !field.type_ref.is_non_null
                            && (referent_name == required.type_name || referent_is_scalar)
                    };

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
                        "either String or Scalar, which can be null if there are no results"
                            .to_string()
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
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("server must provide a `PageInfo`"));
        assert_eq!(all[0].span.start, 0);
        assert_eq!(all[0].span.end, 1);
    }

    #[test]
    fn custom_scalar_cursor_is_valid() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        let schema = r"
            scalar Cursor
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: Boolean!
                startCursor: Cursor
                endCursor: Cursor
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty(), "expected no diagnostics, got {all:?}");
    }

    #[test]
    fn id_cursor_is_valid() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        // `ID` is a built-in scalar, so it should also be accepted.
        let schema = r"
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: Boolean!
                startCursor: ID
                endCursor: ID
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty(), "expected no diagnostics, got {all:?}");
    }

    #[test]
    fn object_cursor_is_invalid() {
        let db = RootDatabase::default();
        let rule = RelayPageInfoRuleImpl;
        // An object type is not a scalar, so it should still be flagged.
        let schema = r"
            type CursorObj { value: String }
            type PageInfo {
                hasPreviousPage: Boolean!
                hasNextPage: Boolean!
                startCursor: CursorObj
                endCursor: CursorObj
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|d| d.message.contains("startCursor")));
        assert!(all.iter().any(|d| d.message.contains("endCursor")));
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
