use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_base_db::{FileId, ProjectFiles};
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
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Step 1: Collect all fragment definitions using per-file cached queries
        // Each file's contribution is cached independently by Salsa
        let doc_ids = project_files.document_file_ids(db).ids(db);
        let mut all_fragments: HashMap<String, Vec<FileId>> = HashMap::new();

        for file_id in doc_ids.iter() {
            // Use per-file lookup for granular caching
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
            let defined = graphql_hir::file_defined_fragment_names(db, *file_id, content, metadata);
            for fragment_name in defined.iter() {
                all_fragments
                    .entry(fragment_name.to_string())
                    .or_default()
                    .push(*file_id);
            }
        }

        // Step 2: Collect all used fragment names using per-file cached queries
        let mut used_fragments: HashSet<String> = HashSet::new();

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
            let used = graphql_hir::file_used_fragment_names(db, *file_id, content, metadata);
            for fragment_name in used.iter() {
                used_fragments.insert(fragment_name.to_string());
            }
        }

        // Step 3: Report unused fragments
        for (fragment_name, file_ids) in &all_fragments {
            if !used_fragments.contains(fragment_name) {
                for file_id in file_ids {
                    let message = format!(
                        "Fragment '{fragment_name}' is defined but never used in any operation"
                    );

                    let diag = LintDiagnostic::new(
                        crate::diagnostics::OffsetRange::new(0, fragment_name.len()),
                        self.default_severity(),
                        message,
                        self.name().to_string(),
                    );

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        diagnostics_by_file
    }
}
