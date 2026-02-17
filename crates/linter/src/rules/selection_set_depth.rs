use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Options for the `selection_set_depth` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SelectionSetDepthOptions {
    /// Maximum allowed depth for selection sets. Defaults to 5.
    pub max_depth: usize,
}

impl Default for SelectionSetDepthOptions {
    fn default() -> Self {
        Self { max_depth: 5 }
    }
}

impl SelectionSetDepthOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that limits the depth of selection set nesting
///
/// Deeply nested queries can cause performance issues on the server.
/// This rule enforces a maximum nesting depth.
pub struct SelectionSetDepthRuleImpl;

impl LintRule for SelectionSetDepthRuleImpl {
    fn name(&self) -> &'static str {
        "selection_set_depth"
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
        let opts = SelectionSetDepthOptions::from_json(options);
        let mut diagnostics = Vec::new();

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
                            check_depth(
                                &selection_set,
                                0,
                                opts.max_depth,
                                op_name.as_deref(),
                                &doc,
                                &mut diagnostics,
                            );
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        let frag_name = frag
                            .fragment_name()
                            .and_then(|fn_| fn_.name())
                            .map(|n| n.text().to_string());
                        if let Some(selection_set) = frag.selection_set() {
                            check_depth(
                                &selection_set,
                                0,
                                opts.max_depth,
                                frag_name.as_deref(),
                                &doc,
                                &mut diagnostics,
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

fn check_depth(
    selection_set: &cst::SelectionSet,
    current_depth: usize,
    max_depth: usize,
    definition_name: Option<&str>,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(nested) = field.selection_set() {
                    let new_depth = current_depth + 1;
                    if new_depth > max_depth {
                        let name_node = field.name();
                        if let Some(name_node) = name_node {
                            let start: usize = name_node.syntax().text_range().start().into();
                            let end: usize = name_node.syntax().text_range().end().into();
                            let def_desc = definition_name.map_or_else(
                                || "anonymous operation".to_string(),
                                |n| format!("'{n}'"),
                            );
                            diagnostics.push(LintDiagnostic::new(
                                doc.span(start, end),
                                LintSeverity::Warning,
                                format!(
                                    "Selection set depth {new_depth} exceeds maximum of {max_depth} in {def_desc}"
                                ),
                                "selection_set_depth",
                            ));
                        }
                    } else {
                        check_depth(
                            &nested,
                            new_depth,
                            max_depth,
                            definition_name,
                            doc,
                            diagnostics,
                        );
                    }
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    // Inline fragments don't add depth, they forward to the same level
                    check_depth(
                        &nested,
                        current_depth,
                        max_depth,
                        definition_name,
                        doc,
                        diagnostics,
                    );
                }
            }
            cst::Selection::FragmentSpread(_) => {
                // Fragment spreads are checked in their own definitions
            }
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
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
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
        let options = serde_json::json!({ "max_depth": max_depth });
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
        assert!(diagnostics[0].message.contains("exceeds maximum"));
    }

    #[test]
    fn test_custom_depth() {
        let diagnostics = check_with_depth("query Q { user { name } }", 1);
        assert!(diagnostics.is_empty());

        let diagnostics = check_with_depth("query Q { user { posts { id } } }", 1);
        assert_eq!(diagnostics.len(), 1);
    }
}
