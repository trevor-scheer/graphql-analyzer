use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::rules_old::{OperationNameSuffixRule, StandaloneDocumentRule};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use graphql_db::{FileContent, FileId, FileMetadata};

/// Trait implementation for `operation_name_suffix` rule
pub struct OperationNameSuffixRuleImpl;

impl LintRule for OperationNameSuffixRuleImpl {
    fn name(&self) -> &'static str {
        "operation_name_suffix"
    }

    fn description(&self) -> &'static str {
        "Requires operation names to have type-specific suffixes (Query, Mutation, Subscription)"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for OperationNameSuffixRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
    ) -> Vec<LintDiagnostic> {
        let parse = graphql_syntax::parse(db, content, metadata);
        if !parse.errors.is_empty() {
            return Vec::new();
        }

        let content_text = content.text(db);
        let file_uri = metadata.uri(db);

        let ctx = crate::context::StandaloneDocumentContext {
            document: content_text.as_ref(),
            file_name: file_uri.as_str(),
            fragments: None,
            parsed: &parse.tree,
        };

        let rule = OperationNameSuffixRule;
        let diagnostics = rule.check(&ctx);
        diagnostics.iter().map(super::convert_diagnostic).collect()
    }
}
