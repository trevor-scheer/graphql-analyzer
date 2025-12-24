use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Trait implementation for `unique_names` rule
pub struct UniqueNamesRuleImpl;

impl LintRule for UniqueNamesRuleImpl {
    fn name(&self) -> &'static str {
        "unique_names"
    }

    fn description(&self) -> &'static str {
        "Ensures operation and fragment names are unique across the project"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl ProjectLintRule for UniqueNamesRuleImpl {
    fn check(
        &self,
        _db: &dyn graphql_hir::GraphQLHirDatabase,
        _project_files: ProjectFiles,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        // TODO: Implement using HIR queries
        tracing::trace!("unique_names rule not yet fully integrated with HIR");
        HashMap::new()
    }
}
