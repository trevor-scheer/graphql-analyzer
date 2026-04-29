use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;
use std::collections::HashSet;

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
        project_files: ProjectFiles,
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

        // Build the project-wide fragment index once per invocation. This
        // lets check_depth inline spreads from sibling files, matching what
        // graphql-depth-limit does when upstream passes all siblings to it.
        let fragment_index = FragmentIndex::build(db, project_files);

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
                                &fragment_index,
                                &mut HashSet::new(),
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
                                &fragment_index,
                                &mut HashSet::new(),
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

/// Project-wide map from fragment name to its file's source text.
///
/// Built once per `check()` invocation so fragment lookups during depth
/// traversal don't need the Salsa database handle inside the recursive walker.
struct FragmentIndex {
    entries: std::collections::HashMap<String, std::sync::Arc<str>>,
}

impl FragmentIndex {
    fn build(db: &dyn graphql_hir::GraphQLHirDatabase, project_files: ProjectFiles) -> Self {
        let all = graphql_hir::all_fragments(db, project_files);
        let mut entries = std::collections::HashMap::new();

        for (name, frag_struct) in all.iter() {
            let Some((content, _metadata)) =
                graphql_base_db::file_lookup(db, project_files, frag_struct.file_id)
            else {
                continue;
            };
            entries.insert(name.to_string(), content.text(db));
        }

        Self { entries }
    }
}

/// Walk the selection set and report fields whose depth exceeds `max_depth`.
///
/// Mirrors `graphql-depth-limit`'s `determineDepth`: each FIELD descent
/// increments `depthSoFar`, and the error is reported at the first field
/// whose `depthSoFar > maxDepth`. Inline fragments forward at the same depth.
/// Fragment spreads are inlined by looking up the spread target in
/// `fragment_index`, matching upstream's behaviour of merging all sibling
/// documents before depth-checking. `visited_spreads` prevents infinite
/// recursion through cyclic fragment references.
#[allow(clippy::too_many_arguments)]
fn check_depth(
    selection_set: &cst::SelectionSet,
    field_depth: usize,
    max_depth: usize,
    ignore: &[String],
    definition_name: Option<&str>,
    doc: &graphql_syntax::DocumentRef<'_>,
    fragment_index: &FragmentIndex,
    visited_spreads: &mut HashSet<String>,
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
                        fragment_index,
                        visited_spreads,
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
                        fragment_index,
                        visited_spreads,
                        diagnostics,
                        reported,
                    );
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                let Some(spread_name) = spread
                    .fragment_name()
                    .and_then(|fn_| fn_.name())
                    .map(|n| n.text().to_string())
                else {
                    continue;
                };

                // Cycle guard: a fragment that directly or transitively spreads
                // itself must not send us into infinite recursion.
                if visited_spreads.contains(&spread_name) {
                    continue;
                }
                visited_spreads.insert(spread_name.clone());

                inline_fragment_spread(
                    &spread_name,
                    fragment_index,
                    field_depth,
                    max_depth,
                    ignore,
                    definition_name,
                    doc,
                    visited_spreads,
                    diagnostics,
                    reported,
                );

                visited_spreads.remove(&spread_name);
            }
        }
    }
}

/// Parse the named fragment's source and walk its selection set at `field_depth`.
///
/// This is the inlining step that makes our depth calculation match upstream's
/// graphql-depth-limit, which merges all sibling documents before checking.
/// The spread site does not add a depth level — the fragment's top-level
/// fields are already at `field_depth`, just as if written inline.
#[allow(clippy::too_many_arguments)]
fn inline_fragment_spread(
    fragment_name: &str,
    fragment_index: &FragmentIndex,
    field_depth: usize,
    max_depth: usize,
    ignore: &[String],
    definition_name: Option<&str>,
    doc: &graphql_syntax::DocumentRef<'_>,
    visited_spreads: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
    reported: &mut bool,
) {
    let Some(source) = fragment_index.entries.get(fragment_name) else {
        return;
    };

    let parse_result = apollo_parser::Parser::new(source).parse();
    let cst = parse_result.document();

    for definition in cst.definitions() {
        if let cst::Definition::FragmentDefinition(frag) = definition {
            let is_target = frag
                .fragment_name()
                .and_then(|fn_| fn_.name())
                .is_some_and(|n| n.text() == fragment_name);
            if !is_target {
                continue;
            }
            if let Some(selection_set) = frag.selection_set() {
                check_depth(
                    &selection_set,
                    field_depth,
                    max_depth,
                    ignore,
                    definition_name,
                    doc,
                    fragment_index,
                    visited_spreads,
                    diagnostics,
                    reported,
                );
            }
            return;
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

    fn check_with_options(source: &str, options: &serde_json::Value) -> Vec<LintDiagnostic> {
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
            Some(options),
        )
    }

    #[test]
    fn test_ignore_skips_field_subtree() {
        // `b` is ignored, so its subtree doesn't contribute to depth at all.
        let opts = serde_json::json!({ "maxDepth": 1, "ignore": ["b"] });
        let diagnostics = check_with_options("query Q { a { b { c { d } } } }", &opts);
        assert!(
            diagnostics.is_empty(),
            "ignored field's subtree should not trip the depth check, got: {diagnostics:?}",
        );
    }

    #[test]
    fn test_ignore_does_not_affect_unrelated_fields() {
        // `b` is ignored but `e` is not — `e`'s subtree still counts.
        let opts = serde_json::json!({ "maxDepth": 1, "ignore": ["b"] });
        let diagnostics =
            check_with_options("query Q { e { f { g } } a { b { c { d } } } }", &opts);
        // `e` (depth 1) → recurse into `f` (depth 2) → exceeds maxDepth=1.
        assert_eq!(diagnostics.len(), 1);
    }
}
