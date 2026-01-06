use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_db::{FileContent, FileId, FileMetadata, ProjectFiles};

/// Lint rule that ensures anonymous operations are only used when they are the sole operation
///
/// Per the GraphQL specification (Section 5.2.2.1 "Lone Anonymous Operation"):
/// > GraphQL allows a short-hand form for defining query operations when only that one
/// > operation exists in the document.
///
/// This rule is spec-compliant and only flags anonymous operations when:
/// - A document contains multiple operations, AND
/// - At least one operation is anonymous
///
/// A single anonymous operation IS valid GraphQL:
/// ```graphql
/// # Valid - single anonymous operation
/// {
///   user {
///     id
///     name
///   }
/// }
/// ```
///
/// Multiple operations with anonymous is invalid:
/// ```graphql
/// # Invalid - anonymous operation with named operation
/// {
///   user { id }
/// }
///
/// query GetPosts {
///   posts { id }
/// }
/// ```
pub struct NoAnonymousOperationsRuleImpl;

impl LintRule for NoAnonymousOperationsRuleImpl {
    fn name(&self) -> &'static str {
        "no_anonymous_operations"
    }

    fn description(&self) -> &'static str {
        "Ensures anonymous operations are only used when they are the sole operation in a document (per GraphQL spec)"
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
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        let parse = graphql_syntax::parse(db, content, metadata);
        if !parse.errors.is_empty() {
            return diagnostics;
        }

        // Check operations in the main document (for .graphql files only)
        // For TS/JS files, parse.tree is the first block and we check all blocks below
        let file_kind = metadata.kind(db);
        if file_kind == graphql_db::FileKind::ExecutableGraphQL
            || file_kind == graphql_db::FileKind::Schema
        {
            let doc_cst = parse.tree.document();
            check_document_operations(&doc_cst, &mut diagnostics);
        }

        // Check operations in extracted blocks (TypeScript/JavaScript)
        for block in &parse.blocks {
            let block_doc = block.tree.document();
            let mut block_diagnostics = Vec::new();
            check_document_operations(&block_doc, &mut block_diagnostics);
            // Add block context to each diagnostic for proper position calculation
            for diag in block_diagnostics {
                diagnostics.push(diag.with_block_context(block.line, block.source.clone()));
            }
        }

        diagnostics
    }
}

/// Check operations in a document and report anonymous operations only when there are multiple operations
fn check_document_operations(doc_cst: &cst::Document, diagnostics: &mut Vec<LintDiagnostic>) {
    // Collect all operations
    let operations: Vec<_> = doc_cst
        .definitions()
        .filter_map(|def| {
            if let cst::Definition::OperationDefinition(op) = def {
                Some(op)
            } else {
                None
            }
        })
        .collect();

    // Per GraphQL spec 5.2.2.1: A single anonymous operation is valid
    // Only flag anonymous operations when there are multiple operations
    if operations.len() <= 1 {
        return;
    }

    // Multiple operations - all anonymous ones should be flagged
    for operation in operations {
        if operation.name().is_none() {
            report_anonymous_operation(&operation, diagnostics);
        }
    }
}

/// Report a diagnostic for an anonymous operation in a multi-operation document
fn report_anonymous_operation(
    operation: &cst::OperationDefinition,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
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
        "Anonymous {operation_type} operation in document with multiple operations. Per GraphQL spec, anonymous operations are only valid when they are the sole operation"
    );

    diagnostics.push(LintDiagnostic::new(
        crate::diagnostics::OffsetRange::new(start_offset, end_offset),
        LintSeverity::Error,
        message,
        "no_anonymous_operations".to_string(),
    ));
}

/// Determine the operation type (query, mutation, or subscription)
fn get_operation_type(operation: &cst::OperationDefinition) -> &'static str {
    operation.operation_type().map_or("query", |op_type| {
        if op_type.query_token().is_some() {
            "query"
        } else if op_type.mutation_token().is_some() {
            "mutation"
        } else if op_type.subscription_token().is_some() {
            "subscription"
        } else {
            "query" // Default fallback
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{
        FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles, RootDatabase,
    };
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_single_anonymous_query_with_keyword_is_valid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // Per GraphQL spec 5.2.2.1, a single anonymous operation IS valid
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Single anonymous operation is valid per spec
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_single_anonymous_query_shorthand_is_valid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // Per GraphQL spec, the shorthand syntax is allowed for single anonymous queries
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Single anonymous operation is valid per spec
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_single_anonymous_mutation_is_valid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // Per GraphQL spec, a single anonymous mutation IS valid
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Single anonymous operation is valid per spec
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_single_anonymous_subscription_is_valid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // Per GraphQL spec, a single anonymous subscription IS valid
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Single anonymous operation is valid per spec
        assert_eq!(diagnostics.len(), 0);
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Fragment definitions should be ignored, query is named
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_anonymous_with_fragment_is_valid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // A document with a single anonymous operation and fragments is valid
        let source = "
fragment UserFields on User {
  id
  name
}

{
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Single anonymous operation with fragments is valid per spec
        assert_eq!(diagnostics.len(), 0);
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_single_anonymous_query_with_variables_is_valid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // A single anonymous operation with variables is valid per spec
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Single anonymous operation is valid per spec
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_anonymous_query_with_named_query_is_invalid() {
        let db = RootDatabase::default();
        let rule = NoAnonymousOperationsRuleImpl;

        // Anonymous operation with another named operation is INVALID
        let source = "
{
  user { id }
}

query GetPosts {
  posts { id }
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

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Anonymous operation in multi-operation document is invalid
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
        assert!(diagnostics[0].message.contains("multiple operations"));
    }
}
