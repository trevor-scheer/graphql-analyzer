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

/// Parse import comments from GraphQL source text.
///
/// Recognizes the format: `# import FragmentName from "path"`
/// Returns a set of imported fragment names.
fn parse_import_comments(source: &str) -> HashSet<String> {
    let mut imported = HashSet::new();

    for line in source.lines() {
        let trimmed = line.trim();
        // Match: # import FragmentName from "..."
        // or:    # import FragmentName from '...'
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix("import") {
                // Must have whitespace after "import"
                if rest.starts_with(char::is_whitespace) {
                    let rest = rest.trim();
                    // Extract the fragment name (everything before "from")
                    if let Some(from_idx) = rest.find(" from ") {
                        let fragment_names_str = &rest[..from_idx];
                        // Support multiple imports: # import A, B from "..."
                        for name in fragment_names_str.split(',') {
                            let name = name.trim();
                            if !name.is_empty() {
                                imported.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    imported
}

/// Collect all fragment spread names and their source positions from a selection set,
/// recursing into nested selection sets.
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

impl StandaloneDocumentLintRule for RequireImportFragmentRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        for doc in parse.documents() {
            // Parse import comments from the source text
            let imported_fragments = parse_import_comments(doc.source);

            // Collect locally defined fragment names (these don't need imports)
            let mut local_fragments = HashSet::new();
            for def in doc.tree.document().definitions() {
                if let cst::Definition::FragmentDefinition(frag) = &def {
                    if let Some(name) = frag.fragment_name().and_then(|fn_| fn_.name()) {
                        local_fragments.insert(name.text().to_string());
                    }
                }
            }

            // Find all fragment spreads across all definitions
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

            // Report spreads that are neither locally defined nor imported
            let mut reported = HashSet::new();
            for (frag_name, start, end) in spreads {
                if local_fragments.contains(&frag_name) {
                    continue;
                }
                if imported_fragments.contains(&frag_name) {
                    continue;
                }
                // Avoid duplicate diagnostics for the same fragment name at the same position
                if !reported.insert((frag_name.clone(), start)) {
                    continue;
                }

                diagnostics.push(
                    LintDiagnostic::new(
                        doc.span(start, end),
                        LintSeverity::Warning,
                        format!(
                            "Fragment '{frag_name}' is used without a corresponding import comment"
                        ),
                        "requireImportFragment",
                    )
                    .with_help(format!(
                        "Add an import comment: # import {frag_name} from \"path/to/file.graphql\""
                    )),
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
        assert!(diagnostics[0]
            .message
            .contains("without a corresponding import"));
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
        assert!(help.contains("# import UserFields"));
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
        assert!(imports.contains("Foo"));
        assert_eq!(imports.len(), 1);
    }

    #[test]
    fn test_parse_import_comments_multiple() {
        let imports = parse_import_comments(r#"# import A, B from "types.graphql""#);
        assert!(imports.contains("A"));
        assert!(imports.contains("B"));
        assert_eq!(imports.len(), 2);
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
