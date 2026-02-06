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

            // Add block context for embedded GraphQL (byte_offset > 0)
            if doc.byte_offset > 0 {
                for diag in doc_diagnostics {
                    diagnostics.push(diag.with_block_context(
                        doc.line_offset,
                        doc.byte_offset,
                        std::sync::Arc::from(doc.source),
                    ));
                }
            } else {
                diagnostics.extend(doc_diagnostics);
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneDocumentLintRule;
    use graphql_base_db::{ExtractionOffset, FileContent, FileId, FileKind, FileMetadata, FileUri};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_empty_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    fn run_rule(db: &RootDatabase, source: &str) -> Vec<LintDiagnostic> {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(source));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
            ExtractionOffset::default(),
        );
        let project_files = create_empty_project_files(db);

        let rule = OperationNameSuffixRuleImpl;
        rule.check(db, file_id, content, metadata, project_files, None)
    }

    #[test]
    fn test_query_with_correct_suffix() {
        let db = RootDatabase::default();
        let source = "query GetUserQuery { user { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_query_without_suffix_warns() {
        let db = RootDatabase::default();
        let source = "query GetUser { user { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("GetUser"));
        assert!(diagnostics[0].message.contains("Query"));
    }

    #[test]
    fn test_mutation_with_correct_suffix() {
        let db = RootDatabase::default();
        let source = "mutation UpdateUserMutation { updateUser { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_mutation_without_suffix_warns() {
        let db = RootDatabase::default();
        let source = "mutation UpdateUser { updateUser { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("UpdateUser"));
        assert!(diagnostics[0].message.contains("Mutation"));
    }

    #[test]
    fn test_subscription_with_correct_suffix() {
        let db = RootDatabase::default();
        let source = "subscription OnUserUpdateSubscription { userUpdated { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_subscription_without_suffix_warns() {
        let db = RootDatabase::default();
        let source = "subscription OnUserUpdate { userUpdated { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("OnUserUpdate"));
        assert!(diagnostics[0].message.contains("Subscription"));
    }

    #[test]
    fn test_anonymous_query_no_warning() {
        let db = RootDatabase::default();
        let source = "{ user { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_anonymous_mutation_no_warning() {
        let db = RootDatabase::default();
        let source = "mutation { updateUser { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_multiple_operations_mixed() {
        let db = RootDatabase::default();
        let source = r"
query GetUserQuery { user { id } }
query FetchPosts { posts { id } }
mutation UpdateUserMutation { updateUser { id } }
";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("FetchPosts"));
    }

    #[test]
    fn test_wrong_suffix_for_operation_type() {
        let db = RootDatabase::default();
        let source = "mutation UpdateUserQuery { updateUser { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Mutation"));
    }

    #[test]
    fn test_shorthand_query_no_warning() {
        let db = RootDatabase::default();
        let source = "query { user { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_suggestion_includes_correct_suffix() {
        let db = RootDatabase::default();
        let source = "query GetUser { user { id } }";

        let diagnostics = run_rule(&db, source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("GetUserQuery"));
    }
}
