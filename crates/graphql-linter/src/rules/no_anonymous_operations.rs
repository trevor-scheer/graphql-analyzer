use crate::context::StandaloneDocumentContext;
use crate::rules::StandaloneDocumentRule;
use apollo_parser::cst::{self, CstNode};
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that enforces all operations have names
///
/// Anonymous operations make tracking, debugging, and performance monitoring difficult.
/// Named operations are essential for APM tools, error tracking, and operational visibility.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - anonymous operation
/// query {
///   user { id }
/// }
///
/// # ✅ Good - named operation
/// query GetUser {
///   user { id }
/// }
/// ```
pub struct NoAnonymousOperationsRule;

impl StandaloneDocumentRule for NoAnonymousOperationsRule {
    fn name(&self) -> &'static str {
        "no_anonymous_operations"
    }

    fn description(&self) -> &'static str {
        "Require all operations to have names for better debugging and monitoring"
    }

    fn check(&self, ctx: &StandaloneDocumentContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let document = ctx.parsed.document();

        for definition in document.definitions() {
            if let cst::Definition::OperationDefinition(operation) = definition {
                // Check if operation has a name
                if operation.name().is_none() {
                    let operation_type_str =
                        operation.operation_type().map_or("query", |op_type| {
                            if op_type.query_token().is_some() {
                                "query"
                            } else if op_type.mutation_token().is_some() {
                                "mutation"
                            } else if op_type.subscription_token().is_some() {
                                "subscription"
                            } else {
                                "query" // default
                            }
                        });

                    // Get the range of the operation keyword (or start of selection set if no keyword)
                    let syntax = operation.syntax();
                    let text_range = syntax.text_range();
                    let start_offset: usize = text_range.start().into();
                    let end_offset: usize =
                        (start_offset + operation_type_str.len()).min(text_range.end().into());

                    let start_pos = offset_to_position(ctx.document, start_offset);
                    let end_pos = offset_to_position(ctx.document, end_offset);

                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        range: Range {
                            start: start_pos,
                            end: end_pos,
                        },
                        message: format!(
                            "Anonymous {operation_type_str} operation. All operations should have names for better debugging and monitoring."
                        ),
                        code: Some(self.name().to_string()),
                        source: "graphql-linter".to_string(),
                        related_info: Vec::new(),
                    });
                }
            }
        }

        diagnostics
    }
}

/// Helper function to convert byte offset to Position
fn offset_to_position(document: &str, offset: usize) -> Position {
    let (line, character) = offset_to_line_col(document, offset);
    Position { line, character }
}

/// Convert a byte offset to a line and column (0-indexed)
fn offset_to_line_col(document: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for ch in document.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }

        current_offset += ch.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_parser::Parser;

    fn parse(source: &str) -> apollo_parser::SyntaxTree {
        Parser::new(source).parse()
    }

    #[test]
    fn test_anonymous_query_triggers_error() {
        let rule = NoAnonymousOperationsRule;
        let source = "query { user { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
    }

    #[test]
    fn test_anonymous_mutation_triggers_error() {
        let rule = NoAnonymousOperationsRule;
        let source = "mutation { createUser(name: \"test\") { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous mutation operation"));
    }

    #[test]
    fn test_anonymous_subscription_triggers_error() {
        let rule = NoAnonymousOperationsRule;
        let source = "subscription { userUpdated { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous subscription operation"));
    }

    #[test]
    fn test_named_query_passes() {
        let rule = NoAnonymousOperationsRule;
        let source = "query GetUser { user { id } }";
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
    fn test_named_mutation_passes() {
        let rule = NoAnonymousOperationsRule;
        let source = "mutation CreateUser { createUser(name: \"test\") { id } }";
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
    fn test_multiple_anonymous_operations() {
        let rule = NoAnonymousOperationsRule;
        let source = r#"
            query { user { id } }
            mutation { createUser(name: "test") { id } }
        "#;
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_mixed_named_and_anonymous() {
        let rule = NoAnonymousOperationsRule;
        let source = r#"
            query GetUser { user { id } }
            mutation { createUser(name: "test") { id } }
        "#;
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Anonymous mutation operation"));
    }

    #[test]
    fn test_fragments_are_ignored() {
        let rule = NoAnonymousOperationsRule;
        let source = r"
            fragment UserFields on User {
                id
                name
            }

            query GetUser {
                user {
                    ...UserFields
                }
            }
        ";
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
    fn test_shorthand_query_syntax() {
        let rule = NoAnonymousOperationsRule;
        // Shorthand query syntax (no 'query' keyword)
        let source = "{ user { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Anonymous query operation"));
    }
}
