use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_db::{FileId, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Trait implementation for `unused_fragments` rule
pub struct UnusedFragmentsRuleImpl;

impl LintRule for UnusedFragmentsRuleImpl {
    fn name(&self) -> &'static str {
        "unused_fragments"
    }

    fn description(&self) -> &'static str {
        "Detects fragment definitions that are never used in any operation"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl ProjectLintRule for UnusedFragmentsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Step 1: Collect all fragment definitions
        let document_files_input = project_files.document_files(db);
        let document_files = document_files_input.files(db);
        let mut all_fragments: HashMap<String, Vec<FileId>> = HashMap::new();

        for (file_id, content, metadata) in document_files.iter() {
            let structure = graphql_hir::file_structure(db, *file_id, *content, *metadata);
            for fragment in &structure.fragments {
                all_fragments
                    .entry(fragment.name.to_string())
                    .or_default()
                    .push(*file_id);
            }
        }

        // Step 2: Collect all used fragment names from operations and fragments
        let mut used_fragments = HashSet::new();

        for (_file_id, content, metadata) in document_files.iter() {
            let parse = graphql_syntax::parse(db, *content, *metadata);

            // Scan operations and fragments in the main AST
            for definition in &parse.ast.definitions {
                match definition {
                    apollo_compiler::ast::Definition::OperationDefinition(operation) => {
                        collect_fragment_spreads(&operation.selection_set, &mut used_fragments);
                    }
                    apollo_compiler::ast::Definition::FragmentDefinition(fragment) => {
                        // Fragments can reference other fragments
                        collect_fragment_spreads(&fragment.selection_set, &mut used_fragments);
                    }
                    _ => {}
                }
            }

            // Also scan operations and fragments in extracted blocks (TypeScript/JavaScript)
            for block in &parse.blocks {
                for definition in &block.ast.definitions {
                    match definition {
                        apollo_compiler::ast::Definition::OperationDefinition(operation) => {
                            collect_fragment_spreads(&operation.selection_set, &mut used_fragments);
                        }
                        apollo_compiler::ast::Definition::FragmentDefinition(fragment) => {
                            // Fragments can reference other fragments
                            collect_fragment_spreads(&fragment.selection_set, &mut used_fragments);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Step 3: Report unused fragments
        for (fragment_name, file_ids) in &all_fragments {
            if !used_fragments.contains(fragment_name) {
                for file_id in file_ids {
                    let message = format!(
                        "Fragment '{fragment_name}' is defined but never used in any operation"
                    );

                    let diag = LintDiagnostic {
                        message,
                        offset_range: crate::diagnostics::OffsetRange {
                            start: 0,
                            end: fragment_name.len(),
                        },
                        severity: self.default_severity(),
                        rule: self.name().to_string(),
                    };

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        diagnostics_by_file
    }
}

/// Recursively collect fragment spread names from a selection set
fn collect_fragment_spreads(
    selections: &[apollo_compiler::ast::Selection],
    fragments: &mut HashSet<String>,
) {
    for selection in selections {
        match selection {
            apollo_compiler::ast::Selection::Field(field) => {
                // Recursively check nested selection sets
                collect_fragment_spreads(&field.selection_set, fragments);
            }
            apollo_compiler::ast::Selection::FragmentSpread(spread) => {
                fragments.insert(spread.fragment_name.to_string());
            }
            apollo_compiler::ast::Selection::InlineFragment(inline) => {
                collect_fragment_spreads(&inline.selection_set, fragments);
            }
        }
    }
}
