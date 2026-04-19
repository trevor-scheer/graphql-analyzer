use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Supported naming styles for filenames
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum FilenameStyle {
    /// `camelCase` (e.g., `getUserById`)
    #[serde(rename = "camelCase")]
    CamelCase,
    /// `PascalCase` (e.g., `GetUserById`)
    #[serde(rename = "PascalCase")]
    PascalCase,
    /// `snake_case` (e.g., `get_user_by_id`)
    #[serde(rename = "snake_case")]
    SnakeCase,
    /// kebab-case (e.g., `get-user-by-id`)
    #[serde(rename = "kebab-case")]
    KebabCase,
    /// Match the exact name used in the document
    #[serde(rename = "matchDocumentStyle")]
    #[default]
    MatchDocumentStyle,
}

/// Per-definition-type configuration
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct DefinitionTypeConfig {
    /// The naming style to enforce
    pub style: FilenameStyle,
    /// Optional suffix that should appear in the filename after the name
    #[serde(default)]
    pub suffix: String,
}

/// Options for the `matchDocumentFilename` rule
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct MatchDocumentFilenameOptions {
    /// Configuration for query operations
    pub query: DefinitionTypeConfig,
    /// Configuration for mutation operations
    pub mutation: DefinitionTypeConfig,
    /// Configuration for subscription operations
    pub subscription: DefinitionTypeConfig,
    /// Configuration for fragment definitions
    pub fragment: DefinitionTypeConfig,
}

impl MatchDocumentFilenameOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces GraphQL operations and fragments match their filename.
///
/// For example, a file named `GetUser.graphql` should contain an operation named
/// `GetUser`. This helps with code organization and makes operations easy to find.
///
/// The rule supports configurable naming styles per definition type (query,
/// mutation, subscription, fragment) and optional suffixes.
///
/// Example:
/// ```graphql
/// # File: GetUser.graphql
/// # Good - operation name matches filename
/// query GetUser {
///   user { id name }
/// }
///
/// # File: GetUser.graphql
/// # Bad - operation name doesn't match filename
/// query FetchUser {
///   user { id name }
/// }
/// ```
pub struct MatchDocumentFilenameRuleImpl;

impl LintRule for MatchDocumentFilenameRuleImpl {
    fn name(&self) -> &'static str {
        "matchDocumentFilename"
    }

    fn description(&self) -> &'static str {
        "Enforces that operation and fragment names match the filename"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for MatchDocumentFilenameRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let opts = MatchDocumentFilenameOptions::from_json(options);

        let uri = metadata.uri(db);
        let uri_str = uri.as_str();

        // Extract filename stem from the URI (strip path and extension)
        let Some(filename_stem) = extract_filename_stem(uri_str) else {
            return diagnostics;
        };

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        for doc in parse.documents() {
            let doc_cst = doc.tree.document();

            for definition in doc_cst.definitions() {
                match &definition {
                    cst::Definition::OperationDefinition(operation) => {
                        if let Some(name) = operation.name() {
                            use super::{get_operation_kind, OperationKind};
                            let op_kind = operation
                                .operation_type()
                                .map_or(OperationKind::Query, |op_type| {
                                    get_operation_kind(&op_type)
                                });

                            let config = match op_kind {
                                OperationKind::Query => &opts.query,
                                OperationKind::Mutation => &opts.mutation,
                                OperationKind::Subscription => &opts.subscription,
                            };

                            let name_text = name.text().to_string();
                            let expected_filename =
                                build_expected_filename(&name_text, config.style, &config.suffix);

                            if expected_filename != filename_stem {
                                let op_type_str = match op_kind {
                                    OperationKind::Query => "query",
                                    OperationKind::Mutation => "mutation",
                                    OperationKind::Subscription => "subscription",
                                };

                                let syntax = name.syntax();
                                let start: usize = syntax.text_range().start().into();
                                let end: usize = syntax.text_range().end().into();

                                diagnostics.push(
                                    LintDiagnostic::warning(
                                        doc.span(start, end),
                                        format!(
                                            "Filename '{filename_stem}' does not match {op_type_str} name '{name_text}'. Expected filename '{expected_filename}'."
                                        ),
                                        "matchDocumentFilename",
                                    )
                                    .with_help(format!(
                                        "Rename the file to '{expected_filename}.graphql' or rename the {op_type_str} to match the filename"
                                    )),
                                );
                            }
                        }
                    }
                    cst::Definition::FragmentDefinition(fragment) => {
                        if let Some(frag_name) = fragment.fragment_name() {
                            if let Some(name) = frag_name.name() {
                                let name_text = name.text().to_string();
                                let expected_filename = build_expected_filename(
                                    &name_text,
                                    opts.fragment.style,
                                    &opts.fragment.suffix,
                                );

                                if expected_filename != filename_stem {
                                    let syntax = name.syntax();
                                    let start: usize = syntax.text_range().start().into();
                                    let end: usize = syntax.text_range().end().into();

                                    diagnostics.push(
                                        LintDiagnostic::warning(
                                            doc.span(start, end),
                                            format!(
                                                "Filename '{filename_stem}' does not match fragment name '{name_text}'. Expected filename '{expected_filename}'."
                                            ),
                                            "matchDocumentFilename",
                                        )
                                        .with_help(format!(
                                            "Rename the file to '{expected_filename}.graphql' or rename the fragment to match the filename"
                                        )),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        diagnostics
    }
}

/// Extract the filename stem (no path, no extension) from a URI string.
///
/// Handles both file URIs (`file:///path/to/File.graphql`) and plain paths.
/// Strips `.graphql` and `.gql` extensions.
fn extract_filename_stem(uri: &str) -> Option<String> {
    // Get the last path segment
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let filename = path.rsplit('/').next()?;

    if filename.is_empty() {
        return None;
    }

    // Strip known GraphQL extensions
    let stem = filename
        .strip_suffix(".graphql")
        .or_else(|| filename.strip_suffix(".gql"))
        .unwrap_or(filename);

    if stem.is_empty() {
        return None;
    }

    Some(stem.to_string())
}

/// Build the expected filename from a definition name, applying the given style
/// and optional suffix.
fn build_expected_filename(name: &str, style: FilenameStyle, suffix: &str) -> String {
    let styled = match style {
        FilenameStyle::MatchDocumentStyle => name.to_string(),
        FilenameStyle::CamelCase => to_camel_case(name),
        FilenameStyle::PascalCase => to_pascal_case(name),
        FilenameStyle::SnakeCase => to_snake_case(name),
        FilenameStyle::KebabCase => to_kebab_case(name),
    };
    format!("{styled}{suffix}")
}

/// Split a name into its word components. Handles `PascalCase`, `camelCase`,
/// `snake_case`, and kebab-case inputs.
fn split_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            // Check if previous was lowercase (camelCase boundary)
            // or if this starts a new word in PascalCase
            let prev_lower = current.chars().last().is_some_and(char::is_lowercase);
            if prev_lower {
                words.push(current.clone());
                current.clear();
            }
            current.push(ch);
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn to_camel_case(name: &str) -> String {
    let words = split_words(name);
    let mut result = String::new();
    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            result.push_str(&word.to_lowercase());
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_uppercase().next().unwrap_or(first));
                result.extend(chars.flat_map(char::to_lowercase));
            }
        }
    }
    result
}

fn to_pascal_case(name: &str) -> String {
    let words = split_words(name);
    let mut result = String::new();
    for word in &words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_uppercase().next().unwrap_or(first));
            result.extend(chars.flat_map(char::to_lowercase));
        }
    }
    result
}

fn to_snake_case(name: &str) -> String {
    let words = split_words(name);
    words
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn to_kebab_case(name: &str) -> String {
    let words = split_words(name);
    words
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneDocumentLintRule;
    use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
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

    fn check_with_uri(source: &str, uri: &str) -> Vec<LintDiagnostic> {
        check_with_uri_and_options(source, uri, None)
    }

    fn check_with_uri_and_options(
        source: &str,
        uri: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = MatchDocumentFilenameRuleImpl;
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new(uri),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);
        rule.check(&db, file_id, content, metadata, project_files, options)
    }

    // --- Filename stem extraction ---

    #[test]
    fn test_extract_filename_stem_graphql() {
        assert_eq!(
            extract_filename_stem("file:///path/to/GetUser.graphql"),
            Some("GetUser".to_string())
        );
    }

    #[test]
    fn test_extract_filename_stem_gql() {
        assert_eq!(
            extract_filename_stem("file:///path/to/GetUser.gql"),
            Some("GetUser".to_string())
        );
    }

    #[test]
    fn test_extract_filename_stem_no_extension() {
        assert_eq!(
            extract_filename_stem("file:///path/to/GetUser"),
            Some("GetUser".to_string())
        );
    }

    #[test]
    fn test_extract_filename_stem_plain_path() {
        assert_eq!(
            extract_filename_stem("GetUser.graphql"),
            Some("GetUser".to_string())
        );
    }

    // --- Naming style conversions ---

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("GetUser"), "getUser");
        assert_eq!(to_camel_case("get_user"), "getUser");
        assert_eq!(to_camel_case("get-user"), "getUser");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("getUser"), "GetUser");
        assert_eq!(to_pascal_case("get_user"), "GetUser");
        assert_eq!(to_pascal_case("get-user"), "GetUser");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("GetUser"), "get_user");
        assert_eq!(to_snake_case("getUser"), "get_user");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("GetUser"), "get-user");
        assert_eq!(to_kebab_case("getUser"), "get-user");
    }

    // --- Default behavior (matchDocumentStyle) ---

    #[test]
    fn test_matching_query_name() {
        let diagnostics = check_with_uri(
            "query GetUser { user { id } }",
            "file:///path/to/GetUser.graphql",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_mismatched_query_name() {
        let diagnostics = check_with_uri(
            "query FetchUser { user { id } }",
            "file:///path/to/GetUser.graphql",
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("GetUser"));
        assert!(diagnostics[0].message.contains("FetchUser"));
    }

    #[test]
    fn test_matching_mutation_name() {
        let diagnostics = check_with_uri(
            "mutation UpdateUser { updateUser { id } }",
            "file:///path/to/UpdateUser.graphql",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_mismatched_mutation_name() {
        let diagnostics = check_with_uri(
            "mutation DeleteUser { deleteUser { id } }",
            "file:///path/to/UpdateUser.graphql",
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("mutation"));
    }

    #[test]
    fn test_matching_subscription_name() {
        let diagnostics = check_with_uri(
            "subscription OnUserUpdate { userUpdated { id } }",
            "file:///path/to/OnUserUpdate.graphql",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_matching_fragment_name() {
        let diagnostics = check_with_uri(
            "fragment UserFields on User { id name }",
            "file:///path/to/UserFields.graphql",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_mismatched_fragment_name() {
        let diagnostics = check_with_uri(
            "fragment UserDetails on User { id name }",
            "file:///path/to/UserFields.graphql",
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("fragment"));
        assert!(diagnostics[0].message.contains("UserDetails"));
    }

    // --- Anonymous operations are ignored ---

    #[test]
    fn test_anonymous_query_ignored() {
        let diagnostics =
            check_with_uri("query { user { id } }", "file:///path/to/GetUser.graphql");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_shorthand_query_ignored() {
        let diagnostics = check_with_uri("{ user { id } }", "file:///path/to/GetUser.graphql");
        assert!(diagnostics.is_empty());
    }

    // --- Style options ---

    #[test]
    fn test_camel_case_style() {
        let options = serde_json::json!({
            "query": { "style": "camelCase" }
        });
        let diagnostics = check_with_uri_and_options(
            "query GetUser { user { id } }",
            "file:///path/to/getUser.graphql",
            Some(&options),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_kebab_case_style() {
        let options = serde_json::json!({
            "query": { "style": "kebab-case" }
        });
        let diagnostics = check_with_uri_and_options(
            "query GetUserById { user { id } }",
            "file:///path/to/get-user-by-id.graphql",
            Some(&options),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_snake_case_style() {
        let options = serde_json::json!({
            "query": { "style": "snake_case" }
        });
        let diagnostics = check_with_uri_and_options(
            "query GetUserById { user { id } }",
            "file:///path/to/get_user_by_id.graphql",
            Some(&options),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_pascal_case_style() {
        let options = serde_json::json!({
            "query": { "style": "PascalCase" }
        });
        let diagnostics = check_with_uri_and_options(
            "query get_user { user { id } }",
            "file:///path/to/GetUser.graphql",
            Some(&options),
        );
        assert!(diagnostics.is_empty());
    }

    // --- Suffix ---

    #[test]
    fn test_suffix_match() {
        let options = serde_json::json!({
            "query": { "style": "matchDocumentStyle", "suffix": "Query" }
        });
        let diagnostics = check_with_uri_and_options(
            "query GetUser { user { id } }",
            "file:///path/to/GetUserQuery.graphql",
            Some(&options),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_suffix_mismatch() {
        let options = serde_json::json!({
            "query": { "style": "matchDocumentStyle", "suffix": "Query" }
        });
        let diagnostics = check_with_uri_and_options(
            "query GetUser { user { id } }",
            "file:///path/to/GetUser.graphql",
            Some(&options),
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("GetUserQuery"));
    }

    #[test]
    fn test_fragment_suffix() {
        let options = serde_json::json!({
            "fragment": { "style": "kebab-case", "suffix": ".fragment" }
        });
        let diagnostics = check_with_uri_and_options(
            "fragment UserFields on User { id name }",
            "file:///path/to/user-fields.fragment.graphql",
            Some(&options),
        );
        assert!(diagnostics.is_empty());
    }

    // --- Multiple definitions ---

    #[test]
    fn test_multiple_operations_one_matches() {
        let diagnostics = check_with_uri(
            "query GetUser { user { id } }\nquery FetchPosts { posts { id } }",
            "file:///path/to/GetUser.graphql",
        );
        // Second operation doesn't match
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("FetchPosts"));
    }

    // --- .gql extension ---

    #[test]
    fn test_gql_extension() {
        let diagnostics = check_with_uri(
            "query GetUser { user { id } }",
            "file:///path/to/GetUser.gql",
        );
        assert!(diagnostics.is_empty());
    }

    // --- Snapshot test ---

    #[test]
    fn test_mismatch_snapshot() {
        let diagnostics = check_with_uri(
            r"
query FetchUser { user { id } }
fragment PostDetails on Post { id title }
",
            "file:///path/to/GetUser.graphql",
        );
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        insta::assert_yaml_snapshot!(messages);
    }
}
