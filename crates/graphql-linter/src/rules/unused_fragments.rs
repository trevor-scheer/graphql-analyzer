use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_db::{FileId, ProjectFiles};
use std::collections::HashMap;

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
        _db: &dyn graphql_hir::GraphQLHirDatabase,
        _project_files: ProjectFiles,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        // TODO: Implement using HIR queries
        tracing::trace!("unused_fragments rule not yet fully integrated with HIR");
        HashMap::new()
    }
}
