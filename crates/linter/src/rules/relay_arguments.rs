use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `relayArguments` rule
///
/// Example configuration:
/// ```yaml
/// lint:
///   rules:
///     # Default: requires both forward and backward pagination args
///     relayArguments: warn
///
///     # Only require one direction (forward OR backward)
///     relayArguments: [warn, { includeBoth: false }]
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RelayArgumentsOptions {
    /// When true (default), requires both forward (`first`/`after`) and
    /// backward (`last`/`before`) pagination arguments. When false, either
    /// pair is sufficient.
    #[serde(rename = "includeBoth")]
    pub include_both: bool,
}

impl Default for RelayArgumentsOptions {
    fn default() -> Self {
        Self { include_both: true }
    }
}

impl RelayArgumentsOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces Relay-compliant pagination arguments on connection fields
///
/// Fields returning a type whose name ends with "Connection" must include the
/// proper pagination arguments: `first`/`after` (forward), `last`/`before`
/// (backward), or both.
pub struct RelayArgumentsRuleImpl;

impl LintRule for RelayArgumentsRuleImpl {
    fn name(&self) -> &'static str {
        "relayArguments"
    }

    fn description(&self) -> &'static str {
        "Enforce Relay-compliant pagination arguments on connection fields"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RelayArgumentsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = RelayArgumentsOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            // Only check object and interface types (they have fields)
            if !matches!(type_def.kind, TypeDefKind::Object | TypeDefKind::Interface) {
                continue;
            }

            for field in &type_def.fields {
                // Check if the field returns a Connection type
                if !field.type_ref.name.ends_with("Connection") {
                    continue;
                }

                let arg_names: Vec<&str> =
                    field.arguments.iter().map(|a| a.name.as_ref()).collect();

                let has_first = arg_names.contains(&"first");
                let has_after = arg_names.contains(&"after");
                let has_last = arg_names.contains(&"last");
                let has_before = arg_names.contains(&"before");

                let has_forward = has_first && has_after;
                let has_backward = has_last && has_before;

                let is_valid = if opts.include_both {
                    has_forward && has_backward
                } else {
                    has_forward || has_backward
                };

                if !is_valid {
                    let start: usize = field.name_range.start().into();
                    let end: usize = field.name_range.end().into();
                    let span = graphql_syntax::SourceSpan {
                        start,
                        end,
                        line_offset: 0,
                        byte_offset: 0,
                        source: None,
                    };

                    // Match graphql-eslint behavior: when neither forward nor
                    // backward pagination is present, emit a single
                    // MISSING_ARGUMENTS diagnostic and stop checking this field.
                    if !has_forward && !has_backward {
                        diagnostics_by_file
                            .entry(field.file_id)
                            .or_default()
                            .push(LintDiagnostic::new(
                                span,
                                LintSeverity::Warning,
                                "A field that returns a Connection type must include forward pagination arguments (`first` and `after`), backward pagination arguments (`last` and `before`), or both.".to_string(),
                                "relayArguments",
                            ));
                        continue;
                    }

                    // Otherwise, with `includeBoth=true`, one pair is present and
                    // we need to flag each missing argument from the other pair
                    // individually. graphql-eslint emits per-argument messages
                    // of the form:
                    //   "Field `X` must contain an argument `Y`, that return Z."
                    // where Z is `Int` for first/last and `String or Scalar`
                    // for after/before.
                    //
                    // TODO(parity): graphql-eslint also validates the *types* of
                    // existing first/after/last/before arguments and emits
                    // "Argument `Y` must return Z." when types don't match. We
                    // currently only check presence, not types.
                    let check_missing = |arg_name: &str,
                                         present: bool,
                                         return_type: &str|
                     -> Option<LintDiagnostic> {
                        if present {
                            return None;
                        }
                        Some(LintDiagnostic::new(
                            span.clone(),
                            LintSeverity::Warning,
                            format!(
                                "Field `{}` must contain an argument `{}`, that return {}.",
                                field.name, arg_name, return_type
                            ),
                            "relayArguments",
                        ))
                    };

                    if opts.include_both || has_first || has_after {
                        if let Some(d) = check_missing("first", has_first, "Int") {
                            diagnostics_by_file
                                .entry(field.file_id)
                                .or_default()
                                .push(d);
                        }
                        if let Some(d) = check_missing("after", has_after, "String or Scalar") {
                            diagnostics_by_file
                                .entry(field.file_id)
                                .or_default()
                                .push(d);
                        }
                    }
                    if opts.include_both || has_last || has_before {
                        if let Some(d) = check_missing("last", has_last, "Int") {
                            diagnostics_by_file
                                .entry(field.file_id)
                                .or_default()
                                .push(d);
                        }
                        if let Some(d) = check_missing("before", has_before, "String or Scalar") {
                            diagnostics_by_file
                                .entry(field.file_id)
                                .or_default()
                                .push(d);
                        }
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

    #[test]
    fn test_connection_with_all_args_no_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: Int, after: String, last: Int, before: String): PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post {
    id: ID!
    title: String
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty(), "Expected no warnings: {all:?}");
    }

    #[test]
    fn test_connection_with_no_args_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts: PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Connection type must include"));
        assert!(all[0].message.contains("`first` and `after`"));
        assert!(all[0].message.contains("`last` and `before`"));
    }

    #[test]
    fn test_connection_with_only_forward_args_include_both_true() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: Int, after: String): PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        // Default: includeBoth = true, so only forward args should warn
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        // graphql-eslint emits one diagnostic per missing argument when one
        // pair is present but the other is missing.
        assert_eq!(
            all.len(),
            2,
            "Should warn for missing `last` and `before`: {all:?}"
        );
        let messages: Vec<&str> = all.iter().map(|d| d.message.as_str()).collect();
        assert!(messages
            .iter()
            .any(|m| m.contains("`last`") && m.contains("Int")));
        assert!(messages
            .iter()
            .any(|m| m.contains("`before`") && m.contains("String or Scalar")));
    }

    #[test]
    fn test_connection_with_only_forward_args_include_both_false() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: Int, after: String): PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let options = serde_json::json!({ "includeBoth": false });
        let diagnostics = rule.check(&db, project_files, Some(&options));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(
            all.is_empty(),
            "Should not warn when includeBoth=false and forward args present: {all:?}"
        );
    }

    #[test]
    fn test_connection_with_only_backward_args_include_both_false() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(last: Int, before: String): PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let options = serde_json::json!({ "includeBoth": false });
        let diagnostics = rule.check(&db, project_files, Some(&options));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(
            all.is_empty(),
            "Should not warn with backward args when includeBoth=false: {all:?}"
        );
    }

    #[test]
    fn test_non_connection_field_no_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    name: String
    posts: [Post]
}

type Post {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(
            all.is_empty(),
            "Should not warn for non-Connection fields: {all:?}"
        );
    }

    #[test]
    fn test_partial_forward_args_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: Int): PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let options = serde_json::json!({ "includeBoth": false });
        let diagnostics = rule.check(&db, project_files, Some(&options));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(
            all.len(),
            1,
            "Should warn when only `first` is present without `after`: {all:?}"
        );
    }

    #[test]
    fn test_interface_with_connection_field() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
interface Node {
    id: ID!
}

interface HasPosts {
    posts: PostConnection
}

type PostConnection {
    edges: [PostEdge]
}

type PostEdge {
    node: Post
    cursor: String
}

type Post implements Node {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(
            all.len(),
            1,
            "Should warn on interface connection fields: {all:?}"
        );
        assert!(all[0].message.contains("Connection type must include"));
    }

    #[test]
    fn test_multiple_connection_fields() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts: PostConnection
    comments(first: Int, after: String, last: Int, before: String): CommentConnection
    followers: FollowerConnection
}

type PostConnection { edges: [Post] }
type CommentConnection { edges: [Comment] }
type FollowerConnection { edges: [User] }
type Post { id: ID! }
type Comment { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(
            all.len(),
            2,
            "Should warn on posts and followers but not comments: {all:?}"
        );
        // The aligned graphql-eslint MISSING_ARGUMENTS message is generic and
        // does not include the field name; both diagnostics share the same text.
        let messages: Vec<&str> = all.iter().map(|d| d.message.as_str()).collect();
        assert!(messages
            .iter()
            .all(|m| m.contains("Connection type must include")));
    }

    #[test]
    fn test_connection_with_extra_args_no_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: Int, after: String, last: Int, before: String, filter: String): PostConnection
}

type PostConnection { edges: [Post] }
type Post { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(
            all.is_empty(),
            "Extra args beyond pagination should not cause warnings: {all:?}"
        );
    }
}
