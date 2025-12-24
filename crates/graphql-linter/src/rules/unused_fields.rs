use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Trait implementation for `unused_fields` rule
pub struct UnusedFieldsRuleImpl;

impl LintRule for UnusedFieldsRuleImpl {
    fn name(&self) -> &'static str {
        "unused_fields"
    }

    fn description(&self) -> &'static str {
        "Detects schema fields that are never used in any operation or fragment"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl ProjectLintRule for UnusedFieldsRuleImpl {
    fn check(
        &self,
        _db: &dyn graphql_hir::GraphQLHirDatabase,
        _project_files: ProjectFiles,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        // TODO: Implement using HIR queries
        tracing::trace!("unused_fields rule not yet fully integrated with HIR");
        HashMap::new()
    }
}
