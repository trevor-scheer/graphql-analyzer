use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::HashSet;

/// Lint rule that requires fragment spreads to have a corresponding import comment.
///
/// When working with multiple GraphQL files, fragment dependencies should be
/// made explicit via import comments rather than relying on implicit global
/// resolution. This makes it clear which fragments a file depends on and where
/// they come from.
///
/// The expected import syntax is:
/// ```graphql
/// # import FragmentName from "path/to/file.graphql"
/// ```
///
/// A default import (`# import 'path'`) is also recognized; it is treated as
/// importing every fragment defined in the referenced file.
///
/// Fragments defined in the same document do not require an import.
pub struct RequireImportFragmentRuleImpl;

impl LintRule for RequireImportFragmentRuleImpl {
    fn name(&self) -> &'static str {
        "requireImportFragment"
    }

    fn description(&self) -> &'static str {
        "Requires fragment spreads to have a corresponding import comment"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// A single parsed `# import …` comment.
///
/// - `names`: `None` means a default import (`# import 'path'`) — every
///   fragment in the referenced file counts as imported.
/// - `names`: `Some(v)` means a named import — only the listed fragments.
/// - `path`: the import path exactly as written (before URI resolution).
#[derive(Debug)]
struct ParsedImport {
    names: Option<Vec<String>>,
    path: String,
}

/// Parse `# import …` comments from GraphQL source text.
///
/// Supports:
/// - Named:   `# import Foo from "path"` / `# import A, B from 'path'`
/// - Default: `# import 'path'`          / `# import "path"`
///
/// Whitespace flexibility mirrors upstream: leading `#` may be followed by any
/// amount of whitespace before `import`, and the same applies inside the
/// statement.
fn parse_import_comments(source: &str) -> Vec<ParsedImport> {
    let mut imports = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix('#') else {
            continue;
        };
        let rest = rest.trim();
        let Some(rest) = rest.strip_prefix("import") else {
            continue;
        };
        // Require at least one whitespace char after the `import` keyword so
        // `#importFoo` is not misidentified.
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim();

        if rest.starts_with('"') || rest.starts_with('\'') {
            // Default import: `# import 'path'`
            if let Some(path) = extract_quoted(rest) {
                imports.push(ParsedImport { names: None, path });
            }
        } else if let Some(from_idx) = rest.find(" from ") {
            // Named import: `# import Foo from 'path'` or `# import A, B from 'path'`
            let names_str = &rest[..from_idx];
            let after_from = rest[from_idx + " from ".len()..].trim();
            if let Some(path) = extract_quoted(after_from) {
                let names: Vec<String> = names_str
                    .split(',')
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect();
                if !names.is_empty() {
                    imports.push(ParsedImport {
                        names: Some(names),
                        path,
                    });
                }
            }
        }
    }

    imports
}

/// Extract the content of the first `"…"` or `'…'` quoted string.
fn extract_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let inner = s.get(1..)?;
    let end = inner.find(quote)?;
    Some(inner[..end].to_string())
}

/// Resolve a possibly-relative import path against the current file's URI.
///
/// The harness registers files under `file:///name` URIs. Import paths use
/// POSIX-style relative references (e.g. `./fragments/foo.gql`). We strip the
/// `file://` scheme, do path arithmetic, and reattach the scheme.
fn resolve_import_path(current_file_uri: &str, import_path: &str) -> String {
    let scheme = "file://";
    let base_path = current_file_uri
        .strip_prefix(scheme)
        .unwrap_or(current_file_uri);

    // Parent directory of the current file.
    let parent = if let Some(slash) = base_path.rfind('/') {
        &base_path[..slash]
    } else {
        ""
    };

    let import_normalized = normalize_path(&format!("{parent}/{import_path}"));
    format!("{scheme}{import_normalized}")
}

/// Normalize a POSIX path: collapse empty segments, resolve `.` and `..`.
fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    format!("/{}", segments.join("/"))
}

/// Collect all fragment names defined across all GraphQL documents in a file.
fn fragment_names_in_file(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let parse = graphql_syntax::parse(db, content, metadata);
    if parse.has_errors() {
        return names;
    }
    for doc in parse.documents() {
        for def in doc.tree.document().definitions() {
            if let cst::Definition::FragmentDefinition(frag) = &def {
                if let Some(name) = frag.fragment_name().and_then(|fn_| fn_.name()) {
                    names.insert(name.text().to_string());
                }
            }
        }
    }
    names
}

/// Collect all fragment spread names and their source positions from a
/// selection set, recursing into nested selection sets.
fn collect_fragment_spreads(
    selection_set: &cst::SelectionSet,
    spreads: &mut Vec<(String, usize, usize)>,
) {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(nested) = field.selection_set() {
                    collect_fragment_spreads(&nested, spreads);
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(name) = spread.fragment_name().and_then(|fn_| fn_.name()) {
                    let frag_name = name.text().to_string();
                    let start: usize = name.syntax().text_range().start().into();
                    let end: usize = name.syntax().text_range().end().into();
                    spreads.push((frag_name, start, end));
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    collect_fragment_spreads(&nested, spreads);
                }
            }
        }
    }
}

/// Check whether `frag_name` is covered by any of the resolved imports.
///
/// For named imports, the fragment must both appear in the import list AND be
/// defined in the referenced file. This matches upstream: `# import Foo from
/// 'bar.gql'` where `bar.gql` does not define `Foo` is treated as if the
/// import doesn't satisfy the requirement.
///
/// For default imports, the fragment must be defined in the referenced file.
fn is_fragment_imported(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    frag_name: &str,
    resolved_uris: &[(Option<Vec<String>>, String)],
    project_files: ProjectFiles,
) -> bool {
    let path_map = project_files.file_path_map(db);
    let uri_to_id = path_map.uri_to_id(db);
    let file_entry_map = project_files.file_entry_map(db);
    let entries = file_entry_map.entries(db);

    for (names, uri) in resolved_uris {
        match names {
            Some(named) => {
                // Named import: the fragment must be listed AND defined in the file.
                if !named.iter().any(|n| n == frag_name) {
                    continue;
                }
                if let Some(file_id) = uri_to_id.get(uri.as_str()) {
                    if let Some(entry) = entries.get(file_id) {
                        let defined =
                            fragment_names_in_file(db, entry.content(db), entry.metadata(db));
                        if defined.contains(frag_name) {
                            return true;
                        }
                        // File is in the project but doesn't define the fragment —
                        // the import points to the wrong file, so it doesn't count.
                    } else {
                        // File is registered by ID but has no entry — treat as unknown.
                        return true;
                    }
                } else {
                    // File is not in the project — we can't validate it, so we trust
                    // the import comment and consider the fragment imported.
                    return true;
                }
            }
            None => {
                // Default import: any fragment in the referenced file is imported.
                if let Some(file_id) = uri_to_id.get(uri.as_str()) {
                    if let Some(entry) = entries.get(file_id) {
                        let defined =
                            fragment_names_in_file(db, entry.content(db), entry.metadata(db));
                        if defined.contains(frag_name) {
                            return true;
                        }
                        // File is in the project but doesn't define this fragment —
                        // the default import doesn't cover it.
                    } else {
                        return true;
                    }
                } else {
                    // File is not in the project — treat as unknown, consider imported.
                    return true;
                }
            }
        }
    }
    false
}

impl StandaloneDocumentLintRule for RequireImportFragmentRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        let current_uri = metadata.uri(db).as_str().to_string();

        for doc in parse.documents() {
            let imports = parse_import_comments(doc.source);

            // Precompute resolved URIs for each import so path resolution
            // happens once rather than once per spread.
            let resolved_uris: Vec<(Option<Vec<String>>, String)> = imports
                .iter()
                .map(|imp| {
                    let uri = resolve_import_path(&current_uri, &imp.path);
                    (imp.names.clone(), uri)
                })
                .collect();

            // Collect locally defined fragment names (no import needed).
            let mut local_fragments = HashSet::new();
            for def in doc.tree.document().definitions() {
                if let cst::Definition::FragmentDefinition(frag) = &def {
                    if let Some(name) = frag.fragment_name().and_then(|fn_| fn_.name()) {
                        local_fragments.insert(name.text().to_string());
                    }
                }
            }

            // Find all fragment spreads across all definitions.
            let mut spreads = Vec::new();
            for def in doc.tree.document().definitions() {
                let selection_set = match &def {
                    cst::Definition::OperationDefinition(op) => op.selection_set(),
                    cst::Definition::FragmentDefinition(frag) => frag.selection_set(),
                    _ => None,
                };
                if let Some(selection_set) = selection_set {
                    collect_fragment_spreads(&selection_set, &mut spreads);
                }
            }

            let mut reported = HashSet::new();
            for (frag_name, start, end) in spreads {
                if local_fragments.contains(&frag_name) {
                    continue;
                }

                if is_fragment_imported(db, &frag_name, &resolved_uris, project_files) {
                    continue;
                }

                if !reported.insert((frag_name.clone(), start)) {
                    continue;
                }

                diagnostics.push(
                    LintDiagnostic::new(
                        doc.span(start, end),
                        LintSeverity::Warning,
                        format!("Expected \"{frag_name}\" fragment to be imported."),
                        "requireImportFragment",
                    )
                    .with_message_id("require-import-fragment")
                    .with_help(format!("Add import expression for \"{frag_name}\".")),
                );
            }
        }

        diagnostics
    }
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

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = RequireImportFragmentRuleImpl;
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);
        rule.check(&db, file_id, content, metadata, project_files, None)
    }

    #[test]
    fn test_fragment_spread_without_import() {
        let diagnostics = check("query GetUser { user { ...UserFields } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("UserFields"));
        assert!(diagnostics[0].message.contains("fragment to be imported"));
    }

    #[test]
    fn test_fragment_spread_with_import() {
        let source = r#"# import UserFields from "user-fields.graphql"
query GetUser { user { ...UserFields } }"#;
        let diagnostics = check(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_locally_defined_fragment_no_import_needed() {
        let source = r"fragment UserFields on User { name }
query GetUser { user { ...UserFields } }";
        let diagnostics = check(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_multiple_imports() {
        let source = r#"# import UserFields from "user.graphql"
# import PostFields from "post.graphql"
query GetFeed { user { ...UserFields } posts { ...PostFields } }"#;
        let diagnostics = check(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comma_separated_imports() {
        let source = r#"# import UserFields, PostFields from "types.graphql"
query GetFeed { user { ...UserFields } posts { ...PostFields } }"#;
        let diagnostics = check(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_mixed_imported_and_missing() {
        let source = r#"# import UserFields from "user.graphql"
query GetFeed { user { ...UserFields } posts { ...PostFields } }"#;
        let diagnostics = check(source);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PostFields"));
    }

    #[test]
    fn test_nested_fragment_spread_without_import() {
        let source = "query GetUser { user { posts { ...PostFields } } }";
        let diagnostics = check(source);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PostFields"));
    }

    #[test]
    fn test_fragment_in_inline_fragment() {
        let source = "query GetUser { user { ... on Admin { ...AdminFields } } }";
        let diagnostics = check(source);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("AdminFields"));
    }

    #[test]
    fn test_no_fragment_spreads() {
        let diagnostics = check("query GetUser { user { id name } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_diagnostic_points_to_fragment_name() {
        let source = "query GetUser { user { ...UserFields } }";
        let diagnostics = check(source);
        assert_eq!(diagnostics.len(), 1);
        let name_text = &source[diagnostics[0].span.start..diagnostics[0].span.end];
        assert_eq!(name_text, "UserFields");
    }

    #[test]
    fn test_help_text_suggests_import() {
        let source = "query GetUser { user { ...UserFields } }";
        let diagnostics = check(source);
        assert_eq!(diagnostics.len(), 1);
        let help = diagnostics[0].help.as_deref().unwrap();
        assert!(help.contains("Add import expression for \"UserFields\""));
    }

    #[test]
    fn test_import_with_single_quotes() {
        let source = r"# import UserFields from 'user.graphql'
query GetUser { user { ...UserFields } }";
        let diagnostics = check(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_fragment_spread_in_fragment_definition() {
        let source = r#"# import AddressFields from "address.graphql"
fragment UserFields on User { name address { ...AddressFields } }"#;
        let diagnostics = check(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_fragment_spread_in_fragment_without_import() {
        let source = "fragment UserFields on User { name address { ...AddressFields } }";
        let diagnostics = check(source);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("AddressFields"));
    }

    #[test]
    fn test_parse_import_comments_basic() {
        let imports = parse_import_comments(r#"# import Foo from "foo.graphql""#);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].names, Some(vec!["Foo".to_string()]));
    }

    #[test]
    fn test_parse_import_comments_multiple() {
        let imports = parse_import_comments(r#"# import A, B from "types.graphql""#);
        assert_eq!(imports.len(), 1);
        assert_eq!(
            imports[0].names,
            Some(vec!["A".to_string(), "B".to_string()])
        );
    }

    #[test]
    fn test_parse_import_comments_default() {
        let imports = parse_import_comments(r#"# import "foo.graphql""#);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].names.is_none());
        assert_eq!(imports[0].path, "foo.graphql");
    }

    #[test]
    fn test_parse_import_comments_non_import_comment() {
        let imports = parse_import_comments("# This is a regular comment");
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_import_comments_empty() {
        let imports = parse_import_comments("");
        assert!(imports.is_empty());
    }
}
