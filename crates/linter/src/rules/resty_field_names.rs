use crate::diagnostics::{rule_doc_url, CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `restyFieldNames` rule
///
/// Example configuration:
/// ```yaml
/// lint:
///   rules:
///     # Default: warns on get, list, post, put, patch, delete, fetch prefixes
///     restyFieldNames: warn
///
///     # Custom prefix list
///     restyFieldNames: [warn, { prefixes: ["get", "list", "fetch"] }]
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RestyFieldNamesOptions {
    /// REST-style prefixes to disallow on field names.
    pub prefixes: Vec<String>,
}

impl Default for RestyFieldNamesOptions {
    fn default() -> Self {
        Self {
            prefixes: vec![
                "get".to_string(),
                "list".to_string(),
                "post".to_string(),
                "put".to_string(),
                "patch".to_string(),
                "delete".to_string(),
                "fetch".to_string(),
            ],
        }
    }
}

impl RestyFieldNamesOptions {
    /// Parse options from a JSON value, falling back to defaults on error
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that warns when field names use REST-style prefixes like `get`, `list`, `post`, etc.
///
/// In GraphQL, fields should be named as nouns rather than using REST-style verb prefixes.
/// For example, prefer `user` over `getUser`, and `users` over `listUsers`.
pub struct RestyFieldNamesRuleImpl;

impl LintRule for RestyFieldNamesRuleImpl {
    fn name(&self) -> &'static str {
        "restyFieldNames"
    }

    fn description(&self) -> &'static str {
        "Warns when field names use REST-style prefixes like get, list, post, etc."
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// Check whether a field name starts with a REST-style prefix.
///
/// Returns `true` if the field name starts with the prefix AND either:
/// - The field name equals the prefix exactly (e.g. "fetch")
/// - The character after the prefix is uppercase (e.g. "getUser", "listItems")
///
/// Returns `false` for words that merely happen to start with the prefix
/// (e.g. "getter", "listing", "postal", "delicate").
fn has_resty_prefix(field_name: &str, prefix: &str) -> bool {
    if !field_name.starts_with(prefix) {
        return false;
    }
    if field_name.len() == prefix.len() {
        return true;
    }
    field_name[prefix.len()..]
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
}

/// Given a field name and the matched prefix, produce a suggested replacement
/// by stripping the prefix and lowercasing the first character.
fn suggest_name(field_name: &str, prefix: &str) -> String {
    if field_name.len() == prefix.len() {
        return field_name.to_string();
    }
    let remainder = &field_name[prefix.len()..];
    let mut chars = remainder.chars();
    match chars.next() {
        Some(first) => {
            let lower_first: String = first.to_lowercase().collect();
            format!("{lower_first}{}", chars.as_str())
        }
        None => field_name.to_string(),
    }
}

impl StandaloneSchemaLintRule for RestyFieldNamesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = RestyFieldNamesOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            if !matches!(
                type_def.kind,
                TypeDefKind::Object | TypeDefKind::Interface | TypeDefKind::InputObject
            ) {
                continue;
            }

            for field in &type_def.fields {
                for prefix in &opts.prefixes {
                    if has_resty_prefix(&field.name, prefix) {
                        let suggestion = suggest_name(&field.name, prefix);
                        let start: usize = field.name_range.start().into();
                        let end: usize = field.name_range.end().into();
                        let span = graphql_syntax::SourceSpan {
                            start,
                            end,
                            line_offset: 0,
                            byte_offset: 0,
                            source: None,
                        };

                        let exact_match = *field.name == *suggestion;
                        let message = if exact_match {
                            format!("Field \"{}\" uses REST-style naming", field.name)
                        } else {
                            format!(
                                "Field \"{}\" uses REST-style naming \u{2014} consider \"{}\" instead",
                                field.name, suggestion,
                            )
                        };

                        let mut diag = LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            message,
                            "restyFieldNames",
                        )
                        .with_message_id("resty-field-names")
                        .with_help(format!(
                            "Drop the `{prefix}` prefix and use a noun-style name."
                        ))
                        .with_url(rule_doc_url("restyFieldNames"));

                        // Offer the noun-style rename as a suggestion (manual
                        // quick-fix). Renames are semantic, so they should not
                        // run via `--fix` automatically.
                        if !exact_match {
                            diag = diag.with_suggestion(CodeSuggestion::replace(
                                format!("Rename to `{suggestion}`"),
                                start,
                                end,
                                suggestion,
                            ));
                        }

                        diagnostics_by_file
                            .entry(type_def.file_id)
                            .or_default()
                            .push(diag);
                        break; // only report once per field
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
            graphql_base_db::ResolvedSchemaFileIds::new(db, Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        )
    }

    fn count_diagnostics(diagnostics: &HashMap<FileId, Vec<LintDiagnostic>>) -> usize {
        diagnostics.values().map(Vec::len).sum()
    }

    #[test]
    fn test_resty_prefixes_flagged() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                getUser: User
                listUsers: [User]
                deleteUser: User
                fetchData: String
                postMessage: String
                putItem: String
                patchRecord: String
            }
            type User {
                id: ID!
                name: String!
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        assert_eq!(count_diagnostics(&diagnostics), 7);
    }

    #[test]
    fn test_valid_names_not_flagged() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                getter: String
                listing: [String]
                postal: String
                delicate: String
                putter: String
                patching: String
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        assert_eq!(count_diagnostics(&diagnostics), 0);
    }

    #[test]
    fn test_exact_prefix_match_flagged() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                fetch: String
                get: String
                list: [String]
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        assert_eq!(count_diagnostics(&diagnostics), 3);
    }

    #[test]
    fn test_custom_prefixes() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                getUser: User
                findUser: User
                searchUsers: [User]
            }
            type User {
                id: ID!
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let options = serde_json::json!({ "prefixes": ["find", "search"] });
        let diagnostics = rule.check(&db, project_files, Some(&options));
        // Only findUser and searchUsers should match, not getUser
        assert_eq!(count_diagnostics(&diagnostics), 2);
    }

    #[test]
    fn test_empty_prefixes_no_diagnostics() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                getUser: User
                listUsers: [User]
            }
            type User {
                id: ID!
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let options = serde_json::json!({ "prefixes": [] });
        let diagnostics = rule.check(&db, project_files, Some(&options));
        assert_eq!(count_diagnostics(&diagnostics), 0);
    }

    #[test]
    fn test_suggestion_format() {
        assert_eq!(suggest_name("getUser", "get"), "user");
        assert_eq!(suggest_name("listUsers", "list"), "users");
        assert_eq!(suggest_name("deleteUser", "delete"), "user");
        assert_eq!(suggest_name("fetchData", "fetch"), "data");
        assert_eq!(suggest_name("fetch", "fetch"), "fetch");
    }

    #[test]
    fn test_has_resty_prefix_logic() {
        assert!(has_resty_prefix("getUser", "get"));
        assert!(has_resty_prefix("listUsers", "list"));
        assert!(has_resty_prefix("fetch", "fetch"));
        assert!(!has_resty_prefix("getter", "get"));
        assert!(!has_resty_prefix("listing", "list"));
        assert!(!has_resty_prefix("postal", "post"));
        assert!(!has_resty_prefix("delicate", "delete"));
        assert!(!has_resty_prefix("user", "get"));
    }

    #[test]
    fn test_interface_fields_checked() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                id: ID!
            }
            interface Node {
                getById: ID!
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        assert_eq!(count_diagnostics(&diagnostics), 1);
    }

    #[test]
    fn test_input_object_fields_checked() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = r"
            type Query {
                id: ID!
            }
            input CreateUserInput {
                getField: String
            }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        assert_eq!(count_diagnostics(&diagnostics), 1);
    }

    #[test]
    fn test_diagnostic_message_format() {
        let db = RootDatabase::default();
        let rule = RestyFieldNamesRuleImpl;
        let schema = "type Query { getUser: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0]
            .message
            .contains("Field \"getUser\" uses REST-style naming"));
        assert!(all[0].message.contains("consider \"user\" instead"));
        assert_eq!(all[0].message_id.as_deref(), Some("resty-field-names"));
        assert!(all[0].help.is_some());
        assert!(all[0].url.is_some());
        assert_eq!(all[0].suggestions.len(), 1);
        assert_eq!(all[0].suggestions[0].fix.edits[0].new_text, "user");
    }
}
