use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;
use std::path::Path;

/// Lint rule that warns when file names don't match the operation or fragment name defined within.
///
/// When a GraphQL file contains a named operation or fragment, the file name should
/// reflect that name for discoverability and consistency. The expected file name style
/// is configurable (`PascalCase`, `camelCase`, kebab-case, or `snake_case`).
///
/// Example:
/// ```graphql
/// # File: GetUser.graphql (PascalCase) or get-user.graphql (kebab-case)
/// query GetUser {
///   user {
///     id
///     name
///   }
/// }
/// ```
pub struct MatchDocumentFilenameRuleImpl;

/// Configuration options for the `matchDocumentFilename` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MatchDocumentFilenameOptions {
    /// The naming style expected for file names
    pub style: NamingStyle,
}

impl Default for MatchDocumentFilenameOptions {
    fn default() -> Self {
        Self {
            style: NamingStyle::PascalCase,
        }
    }
}

/// The naming style for file names derived from operation/fragment names
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::enum_variant_names)]
pub enum NamingStyle {
    PascalCase,
    CamelCase,
    KebabCase,
    SnakeCase,
}

impl LintRule for MatchDocumentFilenameRuleImpl {
    fn name(&self) -> &'static str {
        "matchDocumentFilename"
    }

    fn description(&self) -> &'static str {
        "Requires file names to match the operation or fragment name defined within"
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

        let opts: MatchDocumentFilenameOptions = options
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        // Extract file stem from URI
        let uri = metadata.uri(db);
        let uri_str = uri.as_str();
        let Some(file_stem) = extract_file_stem(uri_str) else {
            return diagnostics;
        };

        // Collect operation and fragment names across all documents
        let mut first_operation_name: Option<(String, graphql_syntax::SourceSpan)> = None;
        let mut first_fragment_name: Option<(String, graphql_syntax::SourceSpan)> = None;

        for doc in parse.documents() {
            let doc_cst = doc.tree.document();

            for definition in doc_cst.definitions() {
                match definition {
                    cst::Definition::OperationDefinition(operation) => {
                        if first_operation_name.is_none() {
                            if let Some(name) = operation.name() {
                                let name_text = name.text().to_string();
                                let syntax = name.syntax();
                                let start: usize = syntax.text_range().start().into();
                                let end: usize = syntax.text_range().end().into();
                                let span = doc.span(start, end);
                                first_operation_name = Some((name_text, span));
                            }
                        }
                    }
                    cst::Definition::FragmentDefinition(fragment) => {
                        if first_fragment_name.is_none() {
                            if let Some(name) = fragment.fragment_name() {
                                if let Some(name_node) = name.name() {
                                    let name_text = name_node.text().to_string();
                                    let syntax = name_node.syntax();
                                    let start: usize = syntax.text_range().start().into();
                                    let end: usize = syntax.text_range().end().into();
                                    let span = doc.span(start, end);
                                    first_fragment_name = Some((name_text, span));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Operations take priority over fragments
        let is_operation = first_operation_name.is_some();
        let Some((definition_name, span)) = first_operation_name.or(first_fragment_name) else {
            return diagnostics;
        };

        let expected_filename = transform_name(&definition_name, &opts.style);

        if file_stem != expected_filename {
            let kind = if is_operation {
                "operation"
            } else {
                "fragment"
            };
            diagnostics.push(LintDiagnostic::new(
                span,
                LintSeverity::Warning,
                format!(
                    "File name '{file_stem}' doesn't match {kind} name '{definition_name}' (expected '{expected_filename}')"
                ),
                "matchDocumentFilename",
            ));
        }

        diagnostics
    }
}

/// Extract the file stem (name without extension) from a URI string.
fn extract_file_stem(uri: &str) -> Option<String> {
    // Strip the file:// scheme if present
    let path_str = uri.strip_prefix("file://").unwrap_or(uri);
    let path = Path::new(path_str);
    path.file_stem().and_then(|s| s.to_str()).map(String::from)
}

/// Transform a `PascalCase` operation/fragment name to the expected file name style.
fn transform_name(name: &str, style: &NamingStyle) -> String {
    match style {
        NamingStyle::PascalCase => name.to_string(),
        NamingStyle::CamelCase => {
            let mut chars = name.chars();
            match chars.next() {
                Some(c) => {
                    let lower: String = c.to_lowercase().collect();
                    format!("{lower}{}", chars.as_str())
                }
                None => String::new(),
            }
        }
        NamingStyle::KebabCase => pascal_to_separated(name, '-'),
        NamingStyle::SnakeCase => pascal_to_separated(name, '_'),
    }
}

/// Convert a `PascalCase` string to a separated lowercase string (kebab-case or `snake_case`).
fn pascal_to_separated(name: &str, separator: char) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push(separator);
            }
            for lower in ch.to_lowercase() {
                result.push(lower);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{
        DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language, ProjectFiles,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    fn check_rule(
        db: &RootDatabase,
        source: &str,
        uri: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let rule = MatchDocumentFilenameRuleImpl;
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(source));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new(uri),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(db);
        rule.check(
            db,
            file_id,
            content,
            metadata,
            project_files,
            options,
        )
    }

    #[test]
    fn test_matching_filename_pascal_case() {
        let db = RootDatabase::default();
        let diagnostics = check_rule(
            &db,
            "query GetUser { user { id } }",
            "file:///project/GetUser.graphql",
            None,
        );
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_mismatched_filename() {
        let db = RootDatabase::default();
        let diagnostics = check_rule(
            &db,
            "query GetUser { user { id } }",
            "file:///project/wrong-name.graphql",
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("wrong-name"));
        assert!(diagnostics[0].message.contains("GetUser"));
        assert_eq!(diagnostics[0].severity, LintSeverity::Warning);
    }

    #[test]
    fn test_multiple_definitions_uses_first_operation() {
        let db = RootDatabase::default();
        let source = r"
fragment UserFields on User { id name }
query GetUser { user { ...UserFields } }
";
        let diagnostics = check_rule(&db, source, "file:///project/GetUser.graphql", None);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_fragment_only_file() {
        let db = RootDatabase::default();
        let diagnostics = check_rule(
            &db,
            "fragment UserFields on User { id name }",
            "file:///project/UserFields.graphql",
            None,
        );
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_fragment_only_file_mismatch() {
        let db = RootDatabase::default();
        let diagnostics = check_rule(
            &db,
            "fragment UserFields on User { id name }",
            "file:///project/WrongName.graphql",
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("fragment"));
        assert!(diagnostics[0].message.contains("UserFields"));
    }

    #[test]
    fn test_kebab_case_style() {
        let db = RootDatabase::default();
        let options = serde_json::json!({ "style": "kebabCase" });
        let diagnostics = check_rule(
            &db,
            "query GetUser { user { id } }",
            "file:///project/get-user.graphql",
            Some(&options),
        );
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_kebab_case_style_mismatch() {
        let db = RootDatabase::default();
        let options = serde_json::json!({ "style": "kebabCase" });
        let diagnostics = check_rule(
            &db,
            "query GetUser { user { id } }",
            "file:///project/GetUser.graphql",
            Some(&options),
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("get-user"));
    }

    #[test]
    fn test_snake_case_style() {
        let db = RootDatabase::default();
        let options = serde_json::json!({ "style": "snakeCase" });
        let diagnostics = check_rule(
            &db,
            "query GetUser { user { id } }",
            "file:///project/get_user.graphql",
            Some(&options),
        );
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_camel_case_style() {
        let db = RootDatabase::default();
        let options = serde_json::json!({ "style": "camelCase" });
        let diagnostics = check_rule(
            &db,
            "query GetUser { user { id } }",
            "file:///project/getUser.graphql",
            Some(&options),
        );
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_anonymous_operation_skipped() {
        let db = RootDatabase::default();
        let diagnostics = check_rule(
            &db,
            "query { user { id } }",
            "file:///project/anything.graphql",
            None,
        );
        // Anonymous operations have no name to compare against
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_transform_name_pascal_case() {
        assert_eq!(
            transform_name("GetUser", &NamingStyle::PascalCase),
            "GetUser"
        );
    }

    #[test]
    fn test_transform_name_camel_case() {
        assert_eq!(
            transform_name("GetUser", &NamingStyle::CamelCase),
            "getUser"
        );
    }

    #[test]
    fn test_transform_name_kebab_case() {
        assert_eq!(
            transform_name("GetUser", &NamingStyle::KebabCase),
            "get-user"
        );
    }

    #[test]
    fn test_transform_name_snake_case() {
        assert_eq!(
            transform_name("GetUser", &NamingStyle::SnakeCase),
            "get_user"
        );
    }

    #[test]
    fn test_snapshot_mismatch() {
        let db = RootDatabase::default();
        let diagnostics = check_rule(
            &db,
            "query GetAllUsers { users { id } }",
            "file:///project/wrong-file.graphql",
            None,
        );
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        insta::assert_yaml_snapshot!(messages);
    }
}
