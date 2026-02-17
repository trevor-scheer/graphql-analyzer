use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Options for the `alphabetize` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AlphabetizeOptions {
    /// Check selection sets for alphabetical order
    pub selections: bool,
    /// Check arguments for alphabetical order
    pub arguments: bool,
    /// Check variable definitions for alphabetical order
    pub variables: bool,
}

impl Default for AlphabetizeOptions {
    fn default() -> Self {
        Self {
            selections: true,
            arguments: false,
            variables: false,
        }
    }
}

impl AlphabetizeOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces alphabetical ordering of selections, arguments, and variables
pub struct AlphabetizeRuleImpl;

impl LintRule for AlphabetizeRuleImpl {
    fn name(&self) -> &'static str {
        "alphabetize"
    }

    fn description(&self) -> &'static str {
        "Enforces alphabetical ordering of fields, arguments, and variables"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for AlphabetizeRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let opts = AlphabetizeOptions::from_json(options);
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
                        if opts.variables {
                            if let Some(var_defs) = op.variable_definitions() {
                                check_variable_order(&var_defs, &doc, &mut diagnostics);
                            }
                        }
                        if let Some(selection_set) = op.selection_set() {
                            check_selection_set_order(
                                &selection_set,
                                &opts,
                                &doc,
                                &mut diagnostics,
                            );
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        if let Some(selection_set) = frag.selection_set() {
                            check_selection_set_order(
                                &selection_set,
                                &opts,
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

fn check_selection_set_order(
    selection_set: &cst::SelectionSet,
    opts: &AlphabetizeOptions,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if opts.selections {
        let mut last_name: Option<String> = None;

        for selection in selection_set.selections() {
            let current_name = match &selection {
                cst::Selection::Field(field) => field
                    .alias()
                    .and_then(|a| a.name())
                    .or_else(|| field.name())
                    .map(|n| n.text().to_string()),
                cst::Selection::FragmentSpread(spread) => spread
                    .fragment_name()
                    .and_then(|fn_| fn_.name())
                    .map(|n| n.text().to_string()),
                cst::Selection::InlineFragment(_) => None, // Skip inline fragments for ordering
            };

            if let Some(ref name) = current_name {
                if let Some(ref prev) = last_name {
                    if name.to_lowercase() < prev.to_lowercase() {
                        let start_offset = match &selection {
                            cst::Selection::Field(f) => f
                                .alias()
                                .and_then(|a| a.name())
                                .or_else(|| f.name())
                                .map(|n| {
                                    let s: usize = n.syntax().text_range().start().into();
                                    let e: usize = n.syntax().text_range().end().into();
                                    (s, e)
                                }),
                            cst::Selection::FragmentSpread(s) => {
                                s.fragment_name().and_then(|fn_| fn_.name()).map(|n| {
                                    let s: usize = n.syntax().text_range().start().into();
                                    let e: usize = n.syntax().text_range().end().into();
                                    (s, e)
                                })
                            }
                            cst::Selection::InlineFragment(_) => None,
                        };

                        if let Some((start, end)) = start_offset {
                            diagnostics.push(LintDiagnostic::new(
                                doc.span(start, end),
                                LintSeverity::Warning,
                                format!("'{name}' should be before '{prev}' (alphabetical order)"),
                                "alphabetize",
                            ));
                        }
                    }
                }
                last_name = Some(name.clone());
            }
        }
    }

    // Recurse into nested selection sets
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if opts.arguments {
                    if let Some(arguments) = field.arguments() {
                        check_argument_order(&arguments, doc, diagnostics);
                    }
                }
                if let Some(nested) = field.selection_set() {
                    check_selection_set_order(&nested, opts, doc, diagnostics);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    check_selection_set_order(&nested, opts, doc, diagnostics);
                }
            }
            cst::Selection::FragmentSpread(_) => {}
        }
    }
}

fn check_argument_order(
    arguments: &cst::Arguments,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut last_name: Option<String> = None;

    for arg in arguments.arguments() {
        if let Some(name_node) = arg.name() {
            let name = name_node.text().to_string();
            if let Some(ref prev) = last_name {
                if name.to_lowercase() < prev.to_lowercase() {
                    let start: usize = name_node.syntax().text_range().start().into();
                    let end: usize = name_node.syntax().text_range().end().into();
                    diagnostics.push(LintDiagnostic::new(
                        doc.span(start, end),
                        LintSeverity::Warning,
                        format!("Argument '{name}' should be before '{prev}' (alphabetical order)"),
                        "alphabetize",
                    ));
                }
            }
            last_name = Some(name);
        }
    }
}

fn check_variable_order(
    var_defs: &cst::VariableDefinitions,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut last_name: Option<String> = None;

    for var_def in var_defs.variable_definitions() {
        if let Some(var) = var_def.variable() {
            if let Some(name_node) = var.name() {
                let name = name_node.text().to_string();
                if let Some(ref prev) = last_name {
                    if name.to_lowercase() < prev.to_lowercase() {
                        let start: usize = name_node.syntax().text_range().start().into();
                        let end: usize = name_node.syntax().text_range().end().into();
                        diagnostics.push(LintDiagnostic::new(
                            doc.span(start, end),
                            LintSeverity::Warning,
                            format!(
                                "Variable '${name}' should be before '${prev}' (alphabetical order)"
                            ),
                            "alphabetize",
                        ));
                    }
                }
                last_name = Some(name);
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
        let db = RootDatabase::default();
        let rule = AlphabetizeRuleImpl;
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
    fn test_alphabetical_selections() {
        let diagnostics = check("query Q { user { age email name } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_non_alphabetical_selections() {
        let diagnostics = check("query Q { user { name age email } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("'age' should be before 'name'"));
    }

    #[test]
    fn test_nested_non_alphabetical() {
        let diagnostics = check("query Q { user { posts { title id } } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("'id' should be before 'title'"));
    }
}
