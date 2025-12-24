use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::rules_old::{RedundantFieldsRule, StandaloneDocumentRule};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use graphql_db::{FileContent, FileId, FileMetadata};

/// Trait implementation for `redundant_fields` rule
pub struct RedundantFieldsRuleImpl;

impl LintRule for RedundantFieldsRuleImpl {
    fn name(&self) -> &'static str {
        "redundant_fields"
    }

    fn description(&self) -> &'static str {
        "Detects fields that are redundant because they are already included in a sibling fragment spread"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for RedundantFieldsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
    ) -> Vec<LintDiagnostic> {
        // Get the parsed syntax tree (cached by Salsa)
        let parse = graphql_syntax::parse(db, content, metadata);
        if !parse.errors.is_empty() {
            return Vec::new();
        }

        let content_text = content.text(db);
        let file_uri = metadata.uri(db);

        // Create old-style context
        let ctx = crate::context::StandaloneDocumentContext {
            document: content_text.as_ref(),
            file_name: file_uri.as_str(),
            fragments: None, // TODO: Wire up DocumentIndex from HIR
            parsed: &parse.tree,
        };

        // Call existing implementation
        let rule = RedundantFieldsRule;
        let diagnostics = rule.check(&ctx);

        // Convert to new format
        diagnostics.iter().map(super::convert_diagnostic).collect()
    }
}
