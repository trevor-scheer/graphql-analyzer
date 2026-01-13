use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_db::{FileContent, FileId, FileMetadata, ProjectFiles};

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
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        // Parse the file (cached by Salsa)
        let parse = graphql_syntax::parse(db, content, metadata);
        if !parse.errors.is_empty() {
            return diagnostics;
        }

        // Walk the CST
        let doc_cst = parse.tree.document();

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

                        diagnostics.push(LintDiagnostic::warning(
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

        diagnostics
    }
}
