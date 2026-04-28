use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::{ArgumentDef, TypeDefKind, TypeDefMap};
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

/// What type a pagination argument is expected to have.
#[derive(Debug, Clone, Copy)]
enum ExpectedType {
    /// Used for `first`/`last`: must be exactly `Int`.
    Int,
    /// Used for `after`/`before`: must be `String` or any user-defined Scalar.
    StringOrScalar,
}

impl ExpectedType {
    fn return_type_label(self) -> &'static str {
        match self {
            ExpectedType::Int => "Int",
            ExpectedType::StringOrScalar => "String or Scalar",
        }
    }
}

/// Built-in GraphQL scalar type names that are always present even when not
/// explicitly defined in the user's schema. The HIR's `schema_types` map only
/// contains user-defined (and extension) types, so we must recognise these
/// as scalars without consulting the map.
const BUILTIN_SCALARS: &[&str] = &["Int", "Float", "String", "Boolean", "ID"];

/// Mirrors graphql-eslint's `isAllowedNonNullType` check: unwraps a single
/// `NonNull` wrapper, rejects `List` types, then verifies the named type matches
/// the expected kind. For `StringOrScalar`, any `Scalar` type is accepted
/// (including built-in scalars like `Float` that may not appear in schema_types).
fn is_allowed_arg_type(
    type_ref: &graphql_hir::TypeRef,
    expected: ExpectedType,
    schema_types: &TypeDefMap,
) -> bool {
    if type_ref.is_list {
        return false;
    }
    let type_name = type_ref.name.as_ref();
    let is_builtin_scalar = BUILTIN_SCALARS.contains(&type_name);
    match expected {
        ExpectedType::Int => type_name == "Int",
        ExpectedType::StringOrScalar => {
            if type_name == "String" || is_builtin_scalar {
                return true;
            }
            schema_types
                .get(&type_ref.name)
                .is_some_and(|t| matches!(t.kind, TypeDefKind::Scalar))
        }
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

                let find_arg = |name: &str| -> Option<&ArgumentDef> {
                    field.arguments.iter().find(|a| a.name.as_ref() == name)
                };

                let first_arg = find_arg("first");
                let after_arg = find_arg("after");
                let last_arg = find_arg("last");
                let before_arg = find_arg("before");

                let has_forward = first_arg.is_some() && after_arg.is_some();
                let has_backward = last_arg.is_some() && before_arg.is_some();

                let field_span = {
                    let start: usize = field.name_range.start().into();
                    let end: usize = field.name_range.end().into();
                    graphql_syntax::SourceSpan {
                        start,
                        end,
                        line_offset: 0,
                        byte_offset: 0,
                        source: None,
                    }
                };

                // Match graphql-eslint behavior: when neither forward nor
                // backward pagination is present, emit a single
                // MISSING_ARGUMENTS diagnostic and stop checking this field.
                if !has_forward && !has_backward {
                    diagnostics_by_file
                        .entry(field.file_id)
                        .or_default()
                        .push(
                            LintDiagnostic::new(
                                field_span,
                                LintSeverity::Warning,
                                "A field that returns a Connection type must include forward pagination arguments (`first` and `after`), backward pagination arguments (`last` and `before`), or both.".to_string(),
                                "relayArguments",
                            )
                            .with_message_id("MISSING_ARGUMENTS"),
                        );
                    continue;
                }

                // Otherwise, run per-argument presence + type checks.
                // graphql-eslint emits one of two messages per argument:
                //   "Field `X` must contain an argument `Y`, that return Z."  (missing)
                //   "Argument `Y` must return Z."                              (mistyped)
                // where Z is `Int` for first/last and `String or Scalar`
                // for after/before.
                let mut check_field =
                    |arg_name: &str, arg: Option<&ArgumentDef>, expected: ExpectedType| {
                        let return_type = expected.return_type_label();
                        let diagnostic = match arg {
                            None => Some(LintDiagnostic::new(
                                field_span.clone(),
                                LintSeverity::Warning,
                                format!(
                                    "Field `{}` must contain an argument `{}`, that return {}.",
                                    field.name, arg_name, return_type
                                ),
                                "relayArguments",
                            )),
                            Some(a)
                                if !is_allowed_arg_type(&a.type_ref, expected, schema_types) =>
                            {
                                let start: usize = a.name_range.start().into();
                                let end: usize = a.name_range.end().into();
                                let span = graphql_syntax::SourceSpan {
                                    start,
                                    end,
                                    line_offset: 0,
                                    byte_offset: 0,
                                    source: None,
                                };
                                Some(LintDiagnostic::new(
                                    span,
                                    LintSeverity::Warning,
                                    format!("Argument `{arg_name}` must return {return_type}."),
                                    "relayArguments",
                                ))
                            }
                            Some(_) => None,
                        };
                        if let Some(d) = diagnostic {
                            diagnostics_by_file
                                .entry(field.file_id)
                                .or_default()
                                .push(d);
                        }
                    };

                if opts.include_both || first_arg.is_some() || after_arg.is_some() {
                    check_field("first", first_arg, ExpectedType::Int);
                    check_field("after", after_arg, ExpectedType::StringOrScalar);
                }
                if opts.include_both || last_arg.is_some() || before_arg.is_some() {
                    check_field("last", last_arg, ExpectedType::Int);
                    check_field("before", before_arg, ExpectedType::StringOrScalar);
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

    #[test]
    fn test_first_with_non_int_type_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: String, after: String, last: Int, before: String): PostConnection
}

type PostConnection { edges: [Post] }
type Post { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        let messages: Vec<&str> = all.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(all.len(), 1, "Expected one warning: {messages:?}");
        assert_eq!(messages[0], "Argument `first` must return Int.");
    }

    #[test]
    fn test_after_with_custom_scalar_no_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
scalar Cursor

type User {
    posts(first: Int, after: Cursor, last: Int, before: Cursor): PostConnection
}

type PostConnection { edges: [Post] }
type Post { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(
            all.is_empty(),
            "Custom scalars are accepted for after/before: {all:?}"
        );
    }

    #[test]
    fn test_after_with_non_scalar_type_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type CursorObj {
    value: String
}

type User {
    posts(first: Int, after: CursorObj, last: Int, before: String): PostConnection
}

type PostConnection { edges: [Post] }
type Post { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        let messages: Vec<&str> = all.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(all.len(), 1, "Expected one warning: {messages:?}");
        assert_eq!(
            messages[0],
            "Argument `after` must return String or Scalar."
        );
    }

    #[test]
    fn test_pagination_args_with_list_types_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: [Int], after: [String], last: Int, before: String): PostConnection
}

type PostConnection { edges: [Post] }
type Post { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        let messages: Vec<&str> = all.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(all.len(), 2, "Expected two warnings: {messages:?}");
        assert!(messages.contains(&"Argument `first` must return Int."));
        assert!(messages.contains(&"Argument `after` must return String or Scalar."));
    }

    #[test]
    fn test_pagination_args_with_non_null_types_no_warning() {
        let db = RootDatabase::default();
        let rule = RelayArgumentsRuleImpl;
        let schema = r"
type User {
    posts(first: Int!, after: String!, last: Int!, before: String!): PostConnection
}

type PostConnection { edges: [Post] }
type Post { id: ID! }
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(
            all.is_empty(),
            "NonNull wrapper around correct type is allowed: {all:?}"
        );
    }
}
