use crate::context::StandaloneDocumentContext;
use crate::rules::StandaloneDocumentRule;
use apollo_parser::cst::{self, CstNode};
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that discourages redundant prefixes in operation names
///
/// GraphQL operations should avoid redundant prefixes like "get", "fetch", "query",
/// "mutation", or "subscription" since the operation type already indicates the action.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - redundant prefixes
/// query GetUser { user { id } }
/// query FetchUsers { users { id } }
/// mutation MutateUser { updateUser { id } }
/// subscription SubscribeToUser { userUpdated { id } }
///
/// # ✅ Good - clear names without redundant prefixes
/// query User { user { id } }
/// query Users { users { id } }
/// mutation UpdateUser { updateUser { id } }
/// subscription UserUpdated { userUpdated { id } }
/// ```
pub struct AvoidOperationNamePrefixRule;

impl StandaloneDocumentRule for AvoidOperationNamePrefixRule {
    fn name(&self) -> &'static str {
        "avoid_operation_name_prefix"
    }

    fn description(&self) -> &'static str {
        "Avoid redundant prefixes in operation names (get, fetch, query, mutation, subscription)"
    }

    fn check(&self, ctx: &StandaloneDocumentContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let document = ctx.parsed.document();

        for definition in document.definitions() {
            if let cst::Definition::OperationDefinition(operation) = definition {
                if let Some(name) = operation.name() {
                    let name_text = name.text().to_string();

                    if let Some((prefix, suggested)) = check_for_redundant_prefix(&name_text) {
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
                                "Operation name '{name_text}' has redundant prefix '{prefix}'. Consider renaming to '{suggested}'."
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

/// Check if a name has a redundant prefix and return the prefix and suggested name
fn check_for_redundant_prefix(name: &str) -> Option<(&'static str, String)> {
    let redundant_prefixes = [
        "Get",
        "Fetch",
        "Query",
        "Mutation",
        "Mutate",
        "Subscribe",
        "Subscription",
    ];

    for prefix in &redundant_prefixes {
        if let Some(remainder) = name.strip_prefix(prefix) {
            // Only flag if there's something after the prefix and it starts with uppercase
            if !remainder.is_empty() && remainder.chars().next().unwrap().is_ascii_uppercase() {
                return Some((prefix, remainder.to_string()));
            }
        }
    }

    None
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
    fn test_get_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
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
        assert!(diagnostics[0].message.contains("redundant prefix 'Get'"));
        assert!(diagnostics[0].message.contains("'User'"));
    }

    #[test]
    fn test_fetch_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
        let source = "query FetchUsers { users { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("redundant prefix 'Fetch'"));
        assert!(diagnostics[0].message.contains("'Users'"));
    }

    #[test]
    fn test_query_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
        let source = "query QueryUser { user { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("redundant prefix 'Query'"));
    }

    #[test]
    fn test_mutation_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
        let source = "mutation MutationCreateUser { createUser { id } }";
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
            .contains("redundant prefix 'Mutation'"));
    }

    #[test]
    fn test_mutate_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
        let source = "mutation MutateUser { updateUser { id } }";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("redundant prefix 'Mutate'"));
    }

    #[test]
    fn test_subscription_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
        let source = "subscription SubscriptionUserUpdated { userUpdated { id } }";
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
            .contains("redundant prefix 'Subscription'"));
    }

    #[test]
    fn test_subscribe_prefix_triggers_warning() {
        let rule = AvoidOperationNamePrefixRule;
        let source = "subscription SubscribeToUser { userUpdated { id } }";
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
            .contains("redundant prefix 'Subscribe'"));
    }

    #[test]
    fn test_good_names_pass() {
        let rule = AvoidOperationNamePrefixRule;
        let source = r"
            query User { user { id } }
            query Users { users { id } }
            mutation CreateUser { createUser { id } }
            mutation UpdateUser { updateUser { id } }
            subscription UserUpdated { userUpdated { id } }
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
    fn test_lowercase_after_prefix_passes() {
        let rule = AvoidOperationNamePrefixRule;
        // "Getaway" should pass because 'a' is lowercase after "Get"
        let source = "query Getaway { getaway { id } }";
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
        let rule = AvoidOperationNamePrefixRule;
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
        let rule = AvoidOperationNamePrefixRule;
        let source = "fragment GetUserFields on User { id name }";
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
    fn test_multiple_violations() {
        let rule = AvoidOperationNamePrefixRule;
        let source = r"
            query GetUser { user { id } }
            query FetchPosts { posts { id } }
            mutation MutateComment { updateComment { id } }
        ";
        let parsed = parse(source);
        let ctx = StandaloneDocumentContext {
            document: source,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics.iter().any(|d| d.message.contains("GetUser")));
        assert!(diagnostics.iter().any(|d| d.message.contains("FetchPosts")));
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("MutateComment")));
    }
}
