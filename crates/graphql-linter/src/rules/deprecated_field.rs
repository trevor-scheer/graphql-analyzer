use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{DocumentSchemaLintRule, LintRule};
use graphql_db::{FileContent, FileId, FileMetadata, ProjectFiles};

/// Trait implementation for `deprecated_field` rule
pub struct DeprecatedFieldRuleImpl;

impl LintRule for DeprecatedFieldRuleImpl {
    fn name(&self) -> &'static str {
        "deprecated_field"
    }

    fn description(&self) -> &'static str {
        "Warns when using fields marked as deprecated in the schema"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl DocumentSchemaLintRule for DeprecatedFieldRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
    ) -> Vec<LintDiagnostic> {
        let parse = graphql_syntax::parse(db, content, metadata);
        if !parse.errors.is_empty() {
            return Vec::new();
        }

        // TODO: Get SchemaIndex from HIR
        // For now, return empty diagnostics until we have HIR support
        tracing::trace!("deprecated_field rule not yet fully integrated with HIR");
        Vec::new()
    }
}
