use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};

/// Lint rule that requires all GraphQL operations to have explicit names
///
/// Anonymous operations are allowed by the GraphQL specification but are
/// discouraged in production code. Named operations provide several benefits:
/// - Better monitoring and debugging (operation names appear in logs and APM tools)
/// - Improved caching strategies in GraphQL clients
/// - Self-documenting code that describes what each operation does
/// - Easier security auditing and operation tracking
///
/// Example:
/// ```graphql
/// # Bad - anonymous operation
/// query {
///   user {
///     id
///     name
///   }
/// }
///
/// # Good - named operation
/// query GetUser {
///   user {
///     id
///     name
///   }
/// }
/// ```
pub struct NoAnonymousOperationsRuleImpl;

impl LintRule for NoAnonymousOperationsRuleImpl {
    fn name(&self) -> &'static str {
        "no_anonymous_operations"
    }

    fn description(&self) -> &'static str {
        "Requires all operations to have explicit names for better monitoring and debugging"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl StandaloneDocumentLintRule for NoAnonymousOperationsRuleImpl {
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
                    check_operation_has_name(&operation, &mut doc_diagnostics);
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

/// Check if an operation has a name, and report a diagnostic if it doesn't
fn check_operation_has_name(
    operation: &cst::OperationDefinition,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Check if the operation has a name
    if operation.name().is_none() {
        // Determine the operation type for the error message
        let operation_type = get_operation_type(operation);

        // Get the position for the diagnostic
        // For anonymous operations, we'll point to the operation type keyword or the selection set
        let (start_offset, end_offset) = operation.operation_type().map_or_else(
            || {
                // If there's no operation type (shorthand query syntax), point to the selection set
                operation.selection_set().map_or((0, 1), |selection_set| {
                    let syntax_node = selection_set.syntax();
                    let start: usize = syntax_node.text_range().start().into();
                    // Just highlight the opening brace
                    (start, start + 1)
                })
            },
            |op_type| {
                // If there's an operation type keyword (query, mutation, subscription), point to it
                let syntax_node = op_type.syntax();
                let start: usize = syntax_node.text_range().start().into();
                let end: usize = syntax_node.text_range().end().into();
                (start, end)
            },
        );

        let message = format!(
            "Anonymous {operation_type} operation. All operations should have explicit names for better monitoring and debugging"
        );

        diagnostics.push(LintDiagnostic::new(
            crate::diagnostics::OffsetRange::new(start_offset, end_offset),
            LintSeverity::Error,
            message,
            "no_anonymous_operations".to_string(),
        ));
    }
}

/// Determine the operation type (query, mutation, or subscription)
fn get_operation_type(operation: &cst::OperationDefinition) -> &'static str {
    use super::{get_operation_kind, OperationKind};
    operation
        .operation_type()
        .map_or("query", |op_type| match get_operation_kind(&op_type) {
            OperationKind::Query => "query",
            OperationKind::Mutation => "mutation",
            OperationKind::Subscription => "subscription",
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_anonymous_query_with_keyword() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
query {
  user {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
        assert_eq!(diagnostics[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_anonymous_query_shorthand() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
{
  user {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
        assert_eq!(diagnostics[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_anonymous_mutation() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
mutation {
  updateUser(id: \"123\", name: \"Alice\") {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous mutation operation"));
        assert_eq!(diagnostics[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_anonymous_subscription() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
subscription {
  messageAdded {
    id
    content
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous subscription operation"));
        assert_eq!(diagnostics[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_named_query() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
query GetUser {
  user {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_named_mutation() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
mutation UpdateUser {
  updateUser(id: \"123\", name: \"Alice\") {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_named_subscription() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
subscription OnMessageAdded {
  messageAdded {
    id
    content
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_multiple_operations_some_anonymous() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
query GetUser {
  user {
    id
  }
}

mutation {
  updateUser(id: \"123\") {
    id
  }
}

query {
  posts {
    id
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Should have 2 diagnostics - one for the anonymous mutation, one for the anonymous query
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("Anonymous mutation")));
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("Anonymous query")));
    }

    #[test]
    fn test_fragment_definitions_ignored() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
fragment UserFields on User {
  id
  name
  email
}

query GetUser {
  user {
    ...UserFields
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Fragment definitions should be ignored, query is named
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_single_anonymous_operation() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
query {
  user {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Even a single anonymous operation should fail (per user's request)
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
    }

    #[test]
    fn test_named_query_with_variables() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
query GetUserById($id: ID!) {
  user(id: $id) {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_anonymous_query_with_variables() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = "
query($id: ID!) {
  user(id: $id) {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
    }

    /// Snapshot test demonstrating insta for diagnostic output
    #[test]
    fn test_anonymous_operations_snapshot() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        let source = r#"
query { users { id } }
mutation { updateUser(id: "1") { id } }
subscription { onUserUpdate { id } }
"#;

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        insta::assert_yaml_snapshot!(messages);
    }
}
