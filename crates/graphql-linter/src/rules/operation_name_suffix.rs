use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};

/// Trait implementation for `operation_name_suffix` rule
///
/// GraphQL best practice recommends operation names end with Query, Mutation, or Subscription.
/// This makes it immediately clear what type of operation is being performed when reading code.
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
        _project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        // Parse the file (cached by Salsa)
        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        // Unified: process all documents (works for both pure GraphQL and TS/JS)
        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            let mut doc_diagnostics = Vec::new();

            for definition in doc_cst.definitions() {
                if let cst::Definition::OperationDefinition(operation) = definition {
                    // Only check named operations
                    if let Some(name) = operation.name() {
                        use super::{get_operation_kind, OperationKind};
                        let name_text = name.text();

                        // Determine the operation type
                        let op_kind = operation
                            .operation_type()
                            .map_or(OperationKind::Query, |op_type| get_operation_kind(&op_type));

                        let expected_suffix = match op_kind {
                            OperationKind::Mutation => "Mutation",
                            OperationKind::Subscription => "Subscription",
                            OperationKind::Query => "Query",
                        };

                        if !name_text.ends_with(expected_suffix) {
                            let syntax = name.syntax();
                            let text_range = syntax.text_range();
                            let start_offset: usize = text_range.start().into();
                            let end_offset: usize = text_range.end().into();

                            doc_diagnostics.push(LintDiagnostic::warning(
                                start_offset,
                                end_offset,
                                format!(
                                    "Operation name '{name_text}' should end with '{expected_suffix}'. Consider renaming to '{name_text}{expected_suffix}'."
                                ),
                                "operation_name_suffix",
                            ));
                        }
                    }
                }
            }

            // Add block context for embedded GraphQL (line_offset > 0)
            if doc.line_offset > 0 {
                for diag in doc_diagnostics {
                    diagnostics.push(
                        diag.with_block_context(doc.line_offset, std::sync::Arc::from(doc.source)),
                    );
                }
            } else {
                diagnostics.extend(doc_diagnostics);
            }
        }

        diagnostics
    }
}
