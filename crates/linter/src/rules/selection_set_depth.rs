use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Options for the `selection_set_depth` rule. Mirrors graphql-eslint's
/// schema, which requires `maxDepth`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSetDepthOptions {
    /// Maximum allowed depth for selection sets.
    pub max_depth: usize,
    /// Field names to ignore from the depth calculation. Matches
    /// `graphql-depth-limit`'s `ignore` option (which graphql-eslint wraps):
    /// when a field's name appears here, the field itself doesn't count as
    /// a depth level and we stop recursing into its selection set. Useful
    /// for "wrapper" fields (e.g. connection edges) that pad depth without
    /// adding query complexity.
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl SelectionSetDepthOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Option<Self> {
        value.and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Lint rule that limits the depth of selection set nesting
///
/// Deeply nested queries can cause performance issues on the server.
/// This rule enforces a maximum nesting depth.
pub struct SelectionSetDepthRuleImpl;

impl LintRule for SelectionSetDepthRuleImpl {
    fn name(&self) -> &'static str {
        "selectionSetDepth"
    }

    fn description(&self) -> &'static str {
        "Limits the depth of selection set nesting to prevent overly complex queries"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for SelectionSetDepthRuleImpl {
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
        // graphql-eslint's schema marks `maxDepth` required — without it the
        // rule is effectively a no-op. Match that behaviour rather than
        // silently picking a default that differs across plugins.
        let Some(opts) = SelectionSetDepthOptions::from_json(options) else {
            return diagnostics;
        };

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            for definition in doc_cst.definitions() {
                match definition {
                    cst::Definition::OperationDefinition(op) => {
                        let op_name = op.name().map(|n| n.text().to_string());
                        if let Some(selection_set) = op.selection_set() {
                            let mut reported = false;
                            check_depth(
                                &selection_set,
                                0,
                                opts.max_depth,
                                &opts.ignore,
                                op_name.as_deref(),
                                &doc,
                                &mut diagnostics,
                                &mut reported,
                            );
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        let frag_name = frag
                            .fragment_name()
                            .and_then(|fn_| fn_.name())
                            .map(|n| n.text().to_string());
                        if let Some(selection_set) = frag.selection_set() {
                            let mut reported = false;
                            check_depth(
                                &selection_set,
                                0,
                                opts.max_depth,
                                &opts.ignore,
                                frag_name.as_deref(),
                                &doc,
                                &mut diagnostics,
                                &mut reported,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        diagnostics
    }
}

/// Walk the selection set and report fields whose depth exceeds `max_depth`.
///
/// Mirrors `graphql-depth-limit`'s `determineDepth`: each FIELD descent
/// increments `depthSoFar`, and the error is reported at the first field
/// whose `depthSoFar > maxDepth`. Inline fragments and fragment spreads do
/// not contribute to depth (graphql-eslint inlines spread fragments as a
/// pre-step; we don't follow spreads here for parity at the per-document
/// level the parity test exercises).
/// Walk the selection set and report fields whose depth exceeds `max_depth`.
///
/// Mirrors `graphql-depth-limit`'s `determineDepth` exactly: each field in
/// `selection_set` has depth `field_depth`. If `field_depth > max_depth`,
/// the rule reports at the field's name. Otherwise we recurse into the
/// field's nested selections with `field_depth + 1`.
///
/// Inline fragments forward at the same depth (they don't add a level).
/// Fragment spreads are not followed for parity at the parity test's
/// per-document level — depth-limit inlines them, but our cross-document
/// linker doesn't here, and the parity fixture has no spreads.
fn check_depth(
    selection_set: &cst::SelectionSet,
    field_depth: usize,
    max_depth: usize,
    ignore: &[String],
    definition_name: Option<&str>,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
    reported: &mut bool,
) {
    for selection in selection_set.selections() {
        if *reported {
            return;
        }
        match selection {
            cst::Selection::Field(field) => {
                let field_name = field.name().map(|n| n.text().to_string());
                // Mirror `graphql-depth-limit`'s `ignore`: when a field's
                // name is in the ignore list it doesn't add a depth level
                // and we don't recurse into it. Useful for connection-
                // wrapper fields (`edges`, `node`) that pad depth without
                // adding query complexity.
                if field_name
                    .as_deref()
                    .is_some_and(|n| ignore.iter().any(|i| i == n))
                {
                    continue;
                }
                if field_depth > max_depth {
                    if let Some(name_node) = field.name() {
                        let start: usize = name_node.syntax().text_range().start().into();
                        let end: usize = name_node.syntax().text_range().end().into();
                        let name_for_message = definition_name.unwrap_or("");
                        diagnostics.push(
                            LintDiagnostic::new(
                                doc.span(start, end),
                                LintSeverity::Warning,
                                format!(
                                    "'{name_for_message}' exceeds maximum operation depth of {max_depth}"
                                ),
                                "selectionSetDepth",
                            )
                            .with_help(
                                "Split the query or extract nested selections into fragments to reduce depth",
                            ),
                        );
                        *reported = true;
                        return;
                    }
                } else if let Some(nested) = field.selection_set() {
                    check_depth(
                        &nested,
                        field_depth + 1,
                        max_depth,
                        ignore,
                        definition_name,
                        doc,
                        diagnostics,
                        reported,
                    );
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    // Inline fragments don't add a level — depth-limit forwards
                    // at the same `depthSoFar`.
                    check_depth(
                        &nested,
                        field_depth,
                        max_depth,
                        ignore,
                        definition_name,
                        doc,
                        diagnostics,
                        reported,
                    );
                }
            }
            cst::Selection::FragmentSpread(_) => {}
        }
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
        check_with_depth(source, 3)
    }

    fn check_with_depth(source: &str, max_depth: usize) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = SelectionSetDepthRuleImpl;
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
        let options = serde_json::json!({ "maxDepth": max_depth });
        rule.check(
            &db,
            file_id,
            content,
            metadata,
            project_files,
            Some(&options),
        )
    }

    #[test]
    fn test_no_options_is_noop() {
        let db = RootDatabase::default();
        let rule = SelectionSetDepthRuleImpl;
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("query Q { a { b { c { d } } } }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);
        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_within_depth_limit() {
        let diagnostics = check("query Q { user { name } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_at_depth_limit() {
        // depth 1: user, depth 2: posts, depth 3: author
        let diagnostics = check("query Q { user { posts { author { name } } } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_exceeds_depth_limit() {
        // depth 1: user, depth 2: posts, depth 3: author, depth 4: friends (exceeds 3)
        let diagnostics = check("query Q { user { posts { author { friends { name } } } } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("exceeds maximum operation depth of"));
    }

    #[test]
    fn test_custom_depth() {
        let diagnostics = check_with_depth("query Q { user { name } }", 1);
        assert!(diagnostics.is_empty());

        let diagnostics = check_with_depth("query Q { user { posts { id } } }", 1);
        assert_eq!(diagnostics.len(), 1);
    }

    fn check_with_options(source: &str, options: serde_json::Value) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = SelectionSetDepthRuleImpl;
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
        rule.check(
            &db,
            file_id,
            content,
            metadata,
            project_files,
            Some(&options),
        )
    }

    #[test]
    fn test_ignore_skips_field_subtree() {
        // `b` is ignored, so its subtree doesn't contribute to depth at all.
        let opts = serde_json::json!({ "maxDepth": 1, "ignore": ["b"] });
        let diagnostics = check_with_options("query Q { a { b { c { d } } } }", opts);
        assert!(
            diagnostics.is_empty(),
            "ignored field's subtree should not trip the depth check, got: {diagnostics:?}",
        );
    }

    #[test]
    fn test_ignore_does_not_affect_unrelated_fields() {
        // `b` is ignored but `e` is not — `e`'s subtree still counts.
        let opts = serde_json::json!({ "maxDepth": 1, "ignore": ["b"] });
        let diagnostics = check_with_options("query Q { e { f { g } } a { b { c { d } } } }", opts);
        // `e` (depth 1) → recurse into `f` (depth 2) → exceeds maxDepth=1.
        assert_eq!(diagnostics.len(), 1);
    }
}
