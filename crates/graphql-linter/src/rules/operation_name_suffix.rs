use crate::context::StandaloneDocumentContext;
use crate::rules::StandaloneDocumentRule;
use apollo_parser::cst::{self, CstNode};
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that enforces operation names end with their operation type
///
/// GraphQL best practice recommends operation names end with Query, Mutation, or Subscription.
/// This makes it immediately clear what type of operation is being performed when reading code.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - operation names don't indicate their type
/// query GetUser { user { id } }
/// mutation CreateUser { createUser { id } }
/// subscription UserUpdated { userUpdated { id } }
///
/// # ✅ Good - operation names end with operation type
/// query GetUserQuery { user { id } }
/// mutation CreateUserMutation { createUser { id } }
/// subscription UserUpdatedSubscription { userUpdated { id } }
/// ```
pub struct OperationNameSuffixRule;

impl StandaloneDocumentRule for OperationNameSuffixRule {
    fn name(&self) -> &'static str {
        "operation_name_suffix"
    }

    fn description(&self) -> &'static str {
        "Require operation names to end with Query, Mutation, or Subscription"
    }

    fn check(&self, ctx: &StandaloneDocumentContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let document = ctx.parsed.document();

        for definition in document.definitions() {
            if let cst::Definition::OperationDefinition(operation) = definition {
                if let Some(name) = operation.name() {
                    let name_text = name.text();
                    let operation_type = operation.operation_type().map_or("query", |op_type| {
                        if op_type.query_token().is_some() {
                            "query"
                        } else if op_type.mutation_token().is_some() {
                            "mutation"
                        } else if op_type.subscription_token().is_some() {
                            "subscription"
                        } else {
                            "query"
                        }
                    });

                    let expected_suffix = match operation_type {
                        "mutation" => "Mutation",
                        "subscription" => "Subscription",
                        _ => "Query", // "query" and any other value defaults to "Query"
                    };

                    if !name_text.ends_with(expected_suffix) {
                        let syntax = name.syntax();
                        let text_range = syntax.text_range();
                        let start_offset: usize = text_range.start().into();
                        let end_offset: usize = text_range.end().into();

                        let start_pos = offset_to_position(ctx.document, start_offset);
                        let end_pos = offset_to_position(ctx.document, end_offset);

                        diagnostics.push(Diagnostic {
                            severity: Severity::Warning,
                            range: Range {
                                start: start_pos,
                                end: end_pos,
                            },
                            message: format!(
                                "Operation name '{name_text}' should end with '{expected_suffix}'. Consider renaming to '{name_text}{expected_suffix}'."
                            ),
                            code: Some(self.name().to_string()),
                            source: "graphql-linter".to_string(),
                            related_info: Vec::new(),
                        });
                    }
                }
            }
        }

        diagnostics
    }
}

/// Helper function to convert byte offset to Position
fn offset_to_position(document: &str, offset: usize) -> Position {
    let (line, character) = graphql_project::offset_to_line_col(document, offset);
    Position { line, character }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_parser::Parser;

    fn parse(source: &str) -> apollo_parser::SyntaxTree {
        Parser::new(source).parse()
    }

    #[test]
    fn test_query_without_suffix_triggers_warning() {
        let rule = OperationNameSuffixRule;
        let source = "query GetUser { user { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0].message.contains("should end with 'Query'"));
        assert!(diagnostics[0].message.contains("GetUserQuery"));
    }

    #[test]
    fn test_mutation_without_suffix_triggers_warning() {
        let rule = OperationNameSuffixRule;
        let source = "mutation CreateUser { createUser(name: \"test\") { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0]
            .message
            .contains("should end with 'Mutation'"));
        assert!(diagnostics[0].message.contains("CreateUserMutation"));
    }

    #[test]
    fn test_subscription_without_suffix_triggers_warning() {
        let rule = OperationNameSuffixRule;
        let source = "subscription UserUpdated { userUpdated { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0]
            .message
            .contains("should end with 'Subscription'"));
        assert!(diagnostics[0].message.contains("UserUpdatedSubscription"));
    }

    #[test]
    fn test_query_with_suffix_passes() {
        let rule = OperationNameSuffixRule;
        let source = "query GetUserQuery { user { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_mutation_with_suffix_passes() {
        let rule = OperationNameSuffixRule;
        let source = "mutation CreateUserMutation { createUser(name: \"test\") { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_subscription_with_suffix_passes() {
        let rule = OperationNameSuffixRule;
        let source = "subscription UserUpdatedSubscription { userUpdated { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_anonymous_operations_ignored() {
        let rule = OperationNameSuffixRule;
        let source = "query { user { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_fragments_ignored() {
        let rule = OperationNameSuffixRule;
        let source = "fragment UserFields on User { id name }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_multiple_operations() {
        let rule = OperationNameSuffixRule;
        let source = r"
            query GetUserQuery { user { id } }
            mutation CreateUser { createUser { id } }
            query GetPosts { posts { id } }
        ";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        // Should flag CreateUser and GetPosts
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().any(|d| d.message.contains("CreateUser")));
        assert!(diagnostics.iter().any(|d| d.message.contains("GetPosts")));
    }
}
