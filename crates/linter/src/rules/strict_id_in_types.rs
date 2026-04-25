use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `strictIdInTypes` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrictIdInTypesOptions {
    /// Acceptable field names for the unique identifier. Defaults to `["id"]`.
    #[serde(default)]
    pub accepted_id_names: Option<Vec<String>>,
    /// Acceptable types (named, non-null) for the unique identifier. Defaults to `["ID"]`.
    #[serde(default)]
    pub accepted_id_types: Option<Vec<String>>,
    /// Type-name and suffix exclusions.
    #[serde(default)]
    pub exceptions: Option<StrictIdInTypesExceptions>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrictIdInTypesExceptions {
    /// Type names to skip entirely.
    #[serde(default)]
    pub types: Option<Vec<String>>,
    /// Type-name suffixes to skip; types whose name ends with any are skipped.
    #[serde(default)]
    pub suffixes: Option<Vec<String>>,
}

struct ResolvedOptions {
    accepted_id_names: Vec<String>,
    accepted_id_types: Vec<String>,
    exception_types: Vec<String>,
    exception_suffixes: Vec<String>,
}

impl ResolvedOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        let parsed: Option<StrictIdInTypesOptions> =
            value.and_then(|v| serde_json::from_value(v.clone()).ok());

        let (names, types, exceptions) = match parsed {
            Some(opts) => (
                opts.accepted_id_names,
                opts.accepted_id_types,
                opts.exceptions,
            ),
            None => (None, None, None),
        };

        let (exception_types, exception_suffixes) = match exceptions {
            Some(e) => (e.types.unwrap_or_default(), e.suffixes.unwrap_or_default()),
            None => (Vec::new(), Vec::new()),
        };

        Self {
            accepted_id_names: names.unwrap_or_else(|| vec!["id".to_string()]),
            accepted_id_types: types.unwrap_or_else(|| vec!["ID".to_string()]),
            exception_types,
            exception_suffixes,
        }
    }
}

/// Lint rule that requires object types to have exactly one non-nullable
/// unique identifier field.
///
/// Mirrors graphql-eslint's `strict-id-in-types` rule: by default each
/// non-root object type must have exactly one field named `id` of type `ID!`.
/// The accepted names, accepted types, and per-type / suffix exceptions are
/// configurable.
pub struct StrictIdInTypesRuleImpl;

impl LintRule for StrictIdInTypesRuleImpl {
    fn name(&self) -> &'static str {
        "strictIdInTypes"
    }

    fn description(&self) -> &'static str {
        "Requires object types to have exactly one non-nullable unique identifier field"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for StrictIdInTypesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = ResolvedOptions::from_json(options);

        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::Object {
                continue;
            }

            if root_type_names.is_root_type(&type_def.name) {
                continue;
            }

            let type_name = type_def.name.as_ref();

            if opts.exception_types.iter().any(|t| t == type_name) {
                continue;
            }
            if opts
                .exception_suffixes
                .iter()
                .any(|suffix| type_name.ends_with(suffix.as_str()))
            {
                continue;
            }

            let valid_id_count = type_def
                .fields
                .iter()
                .filter(|f| {
                    let name_ok = opts
                        .accepted_id_names
                        .iter()
                        .any(|n| n.as_str() == f.name.as_ref());
                    // graphql-eslint requires `NonNullType<NamedType>` exactly: no list wrappers.
                    let type_ok = f.type_ref.is_non_null
                        && !f.type_ref.is_list
                        && opts
                            .accepted_id_types
                            .iter()
                            .any(|t| t.as_str() == f.type_ref.name.as_ref());
                    name_ok && type_ok
                })
                .count();

            if valid_id_count == 1 {
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

            let names_label = if opts.accepted_id_names.len() > 1 {
                "names"
            } else {
                "name"
            };
            let types_label = if opts.accepted_id_types.len() > 1 {
                "types"
            } else {
                "type"
            };
            let names_joined = english_join(&opts.accepted_id_names);
            let types_joined = english_join(&opts.accepted_id_types);

            diagnostics_by_file
                .entry(type_def.file_id)
                .or_default()
                .push(
                    LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!(
                            "type `{type_name}` must have exactly one non-nullable unique identifier.\nAccepted {names_label}: {names_joined}.\nAccepted {types_label}: {types_joined}."
                        ),
                        "strictIdInTypes",
                    )
                    .with_help(
                        "Add a single non-nullable identifier field so this type can be uniquely identified and cached",
                    ),
                );
        }

        diagnostics_by_file
    }
}

/// Join words with backtick-quoted entries using `Intl.ListFormat`-style
/// disjunction (`a`, `a or b`, `a, b, or c`).
fn english_join(words: &[String]) -> String {
    match words.len() {
        0 => String::new(),
        1 => format!("`{}`", words[0]),
        2 => format!("`{}` or `{}`", words[0], words[1]),
        _ => {
            let head = words[..words.len() - 1]
                .iter()
                .map(|w| format!("`{w}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{head}, or `{}`", words[words.len() - 1])
        }
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
    fn test_type_with_id() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_type_without_id() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { name: String! email: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("`User`"));
        assert!(all[0]
            .message
            .contains("must have exactly one non-nullable unique identifier"));
        assert!(all[0].message.contains("Accepted name: `id`"));
        assert!(all[0].message.contains("Accepted type: `ID`"));
    }

    #[test]
    fn test_query_type_excluded() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type Query { users: [User!]! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_nullable_id_is_not_valid() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { id: ID name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("`User`"));
    }

    #[test]
    fn test_wrong_id_type_is_not_valid() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { id: String! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_duplicate_ids_flagged() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { id: ID! _id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "acceptedIdNames": ["id", "_id"] });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("`User`"));
        assert!(all[0].message.contains("Accepted names: `id` or `_id`"));
    }

    #[test]
    fn test_accepted_id_names_option() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { _id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "acceptedIdNames": ["_id"] });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_accepted_id_types_option() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { id: UUID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "acceptedIdTypes": ["UUID"] });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_exception_types_skipped() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type Error { message: String! }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "exceptions": { "types": ["Error"] } });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_exception_suffixes_skipped() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type CreateUserPayload { data: String! }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "exceptions": { "suffixes": ["Payload"] } });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_no_exception_match_still_flags() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { name: String! }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({
            "exceptions": { "types": ["Error"], "suffixes": ["Payload"] }
        });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("`User`"));
    }
}
