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
        "noAnonymousOperations"
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

            for definition in doc_cst.definitions() {
                if let cst::Definition::OperationDefinition(operation) = definition {
                    check_operation_has_name(&operation, &doc, &mut diagnostics);
                }
            }
        }

        diagnostics
    }
}

/// Check if an operation has a name, and report a diagnostic if it doesn't
fn check_operation_has_name(
    operation: &cst::OperationDefinition,
    doc: &graphql_syntax::DocumentRef<'_>,
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

        // Message format mirrors `@graphql-eslint/eslint-plugin`'s
        // `no-anonymous-operations` so the two plugins emit equivalent text.
        let message = format!(
            "Anonymous GraphQL operations are forbidden. Make sure to name your {operation_type}!"
        );

        // Mirror upstream's suggest: use the first selection's alias/name (if
        // it's a Field) as the suggested name; otherwise fall back to the
        // operation kind (`query`/`mutation`/`subscription`).
        let suggested_name = operation
            .selection_set()
            .and_then(|ss| ss.selections().next())
            .and_then(|sel| match sel {
                cst::Selection::Field(field) => field
                    .alias()
                    .and_then(|a| a.name())
                    .or_else(|| field.name())
                    .map(|n| n.text().to_string()),
                _ => None,
            })
            .unwrap_or_else(|| operation_type.to_string());

        // Mirror upstream's fix:
        //   has-keyword:   insertTextAfter(opTypeKeyword, ` ${suggestedName}`)
        //   shorthand `{`: insertTextBefore(`{`, `query ${suggestedName} `)
        // Both translate to a zero-width replacement at the right offset.
        let suggestion = if let Some(op_type) = operation.operation_type() {
            // hasQueryKeyword branch
            let op_type_end: usize = op_type.syntax().text_range().end().into();
            crate::diagnostics::CodeSuggestion::replace(
                format!("Rename to `{suggested_name}`"),
                op_type_end,
                op_type_end,
                format!(" {suggested_name}"),
            )
        } else if let Some(ss) = operation.selection_set() {
            // shorthand `{`: insert before opening brace
            let brace_start: usize = ss.syntax().text_range().start().into();
            crate::diagnostics::CodeSuggestion::replace(
                format!("Rename to `{suggested_name}`"),
                brace_start,
                brace_start,
                format!("query {suggested_name} "),
            )
        } else {
            crate::diagnostics::CodeSuggestion::replace(
                format!("Rename to `{suggested_name}`"),
                start_offset,
                start_offset,
                format!("query {suggested_name} "),
            )
        };

        diagnostics.push(
            LintDiagnostic::new(
                doc.span(start_offset, end_offset),
                LintSeverity::Error,
                message,
                "noAnonymousOperations",
            )
            .with_message_id("no-anonymous-operations")
            .with_help("Add a name to your operation, e.g. 'query MyQuery { ... }'")
            .with_suggestion(suggestion),
        );
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
    use graphql_base_db::{
        DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language, ProjectFiles,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(
            db,
            schema_file_ids,
            document_file_ids,
            graphql_base_db::ResolvedSchemaFileIds::new(db, std::sync::Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        )
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous GraphQL operations are forbidden. Make sure to name your query"));
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous GraphQL operations are forbidden. Make sure to name your query"));
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(
            "Anonymous GraphQL operations are forbidden. Make sure to name your mutation"
        ));
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(
            "Anonymous GraphQL operations are forbidden. Make sure to name your subscription"
        ));
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Should have 2 diagnostics - one for the anonymous mutation, one for the anonymous query
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("name your mutation")));
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("name your query")));
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Even a single anonymous operation should fail (per user's request)
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous GraphQL operations are forbidden. Make sure to name your query"));
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous GraphQL operations are forbidden. Make sure to name your query"));
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        insta::assert_yaml_snapshot!(messages);
    }
}
