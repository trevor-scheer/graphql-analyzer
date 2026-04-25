use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Selection-set owners that `alphabetize.selections` may restrict to. Mirrors
/// graphql-eslint's `selectionsEnum` (`OperationDefinition`, `FragmentDefinition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SelectionsOwner {
    OperationDefinition,
    FragmentDefinition,
}

/// `selections` accepts either a boolean (legacy) or an array of owner kinds
/// (matching graphql-eslint). `true` is treated as "both owner kinds enabled".
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SelectionsConfig {
    Bool(bool),
    Owners(Vec<SelectionsOwner>),
}

impl SelectionsConfig {
    fn includes(&self, owner: SelectionsOwner) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Owners(list) => list.contains(&owner),
        }
    }
}

impl Default for SelectionsConfig {
    fn default() -> Self {
        Self::Bool(true)
    }
}

/// Options for the `alphabetize` rule
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AlphabetizeOptions {
    /// Check selection sets for alphabetical order. Either a boolean or an
    /// array of selection-set owner kinds (`OperationDefinition`,
    /// `FragmentDefinition`).
    pub selections: SelectionsConfig,
    /// Check arguments for alphabetical order
    pub arguments: bool,
    /// Check variable definitions for alphabetical order
    pub variables: bool,
}

impl AlphabetizeOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    fn check_owner(&self, owner: SelectionsOwner) -> bool {
        self.selections.includes(owner)
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
                        let scan = opts.check_owner(SelectionsOwner::OperationDefinition);
                        if let Some(selection_set) = op.selection_set() {
                            check_selection_set_order(
                                &selection_set,
                                &opts,
                                scan,
                                &doc,
                                &mut diagnostics,
                            );
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        let scan = opts.check_owner(SelectionsOwner::FragmentDefinition);
                        if let Some(selection_set) = frag.selection_set() {
                            check_selection_set_order(
                                &selection_set,
                                &opts,
                                scan,
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

#[derive(Debug, Clone, Copy)]
enum SelectionKind {
    Field,
    FragmentSpread,
}

impl SelectionKind {
    fn label(self) -> &'static str {
        match self {
            SelectionKind::Field => "field",
            SelectionKind::FragmentSpread => "fragment spread",
        }
    }
}

fn check_selection_set_order(
    selection_set: &cst::SelectionSet,
    opts: &AlphabetizeOptions,
    scan_selections: bool,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if scan_selections {
        let mut last: Option<(String, SelectionKind)> = None;

        for selection in selection_set.selections() {
            let current = match &selection {
                cst::Selection::Field(field) => field
                    .alias()
                    .and_then(|a| a.name())
                    .or_else(|| field.name())
                    .map(|n| (n.text().to_string(), SelectionKind::Field)),
                cst::Selection::FragmentSpread(spread) => spread
                    .fragment_name()
                    .and_then(|fn_| fn_.name())
                    .map(|n| (n.text().to_string(), SelectionKind::FragmentSpread)),
                cst::Selection::InlineFragment(_) => None, // Inline fragments don't have a name to order by
            };

            if let Some((name, curr_kind)) = current {
                if let Some((prev_name, prev_kind)) = &last {
                    if name.to_lowercase() < prev_name.to_lowercase() {
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
                            diagnostics.push(
                                LintDiagnostic::new(
                                    doc.span(start, end),
                                    LintSeverity::Warning,
                                    format!(
                                        "{curr_label} \"{name}\" should be before {prev_label} \"{prev_name}\"",
                                        curr_label = curr_kind.label(),
                                        prev_label = prev_kind.label(),
                                    ),
                                    "alphabetize",
                                )
                                .with_help(
                                    "Reorder selections alphabetically by their response name",
                                ),
                            );
                        }
                    }
                }
                last = Some((name, curr_kind));
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
                    check_selection_set_order(&nested, opts, scan_selections, doc, diagnostics);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    check_selection_set_order(&nested, opts, scan_selections, doc, diagnostics);
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
                    diagnostics.push(
                        LintDiagnostic::new(
                            doc.span(start, end),
                            LintSeverity::Warning,
                            format!("argument \"{name}\" should be before argument \"{prev}\""),
                            "alphabetize",
                        )
                        .with_help("Reorder arguments alphabetically by name"),
                    );
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
                        diagnostics.push(
                            LintDiagnostic::new(
                                doc.span(start, end),
                                LintSeverity::Warning,
                                format!("variable \"{name}\" should be before variable \"{prev}\""),
                                "alphabetize",
                            )
                            .with_help("Reorder variable definitions alphabetically by name"),
                        );
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
            .contains("field \"age\" should be before field \"name\""));
    }

    #[test]
    fn test_nested_non_alphabetical() {
        let diagnostics = check("query Q { user { posts { title id } } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("field \"id\" should be before field \"title\""));
    }

    #[test]
    fn test_mixed_field_after_fragment_spread() {
        // Fragment spread `Zed` then field `age` — current is field, previous is fragment spread.
        let diagnostics = check("query Q { user { ...Zed age } }");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "field \"age\" should be before fragment spread \"Zed\""
        );
    }

    #[test]
    fn test_mixed_fragment_spread_after_field() {
        // Field `name` then fragment spread `Avatar` — current is fragment spread, previous is field.
        let diagnostics = check("query Q { user { name ...Avatar } }");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "fragment spread \"Avatar\" should be before field \"name\""
        );
    }

    fn check_with_options(source: &str, options: &serde_json::Value) -> Vec<LintDiagnostic> {
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
    fn test_selections_array_only_operation_definition() {
        // With `selections: ["OperationDefinition"]`, fragment definitions
        // are NOT scanned for selection-set ordering.
        let opts = serde_json::json!({ "selections": ["OperationDefinition"] });
        let source = "fragment F on User { name age id }\nquery Q { user { name age id } }\n";
        let diagnostics = check_with_options(source, &opts);
        assert_eq!(diagnostics.len(), 1, "expected only the query to fire");
        assert!(diagnostics[0]
            .message
            .contains("field \"age\" should be before field \"name\""));
    }

    #[test]
    fn test_selections_array_only_fragment_definition() {
        let opts = serde_json::json!({ "selections": ["FragmentDefinition"] });
        let source = "fragment F on User { name age id }\nquery Q { user { name age id } }\n";
        let diagnostics = check_with_options(source, &opts);
        assert_eq!(diagnostics.len(), 1, "expected only the fragment to fire");
    }

    #[test]
    fn test_selections_array_both_kinds() {
        let opts = serde_json::json!({
            "selections": ["OperationDefinition", "FragmentDefinition"]
        });
        let source = "fragment F on User { name age id }\nquery Q { user { name age id } }\n";
        let diagnostics = check_with_options(source, &opts);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_selections_false_disables_check() {
        let opts = serde_json::json!({ "selections": false });
        let diagnostics = check_with_options("query Q { user { name age } }", &opts);
        assert!(diagnostics.is_empty());
    }
}
