//! Verbatim port of `@graphql-eslint`'s `no-anonymous-operations` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts>

use super::harness::{Case, ExpectedError, ExpectedSuggestion};
use crate::rules::no_anonymous_operations::NoAnonymousOperationsRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L5>
#[test]
fn valid_l5_named_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L5",
        super::UPSTREAM_SHA,
    ))
    .code("query myQuery { a }")
    .run_against_standalone_document(NoAnonymousOperationsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L5>
#[test]
fn valid_l5_named_mutation() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L5",
        super::UPSTREAM_SHA,
    ))
    .code("mutation doSomething { a }")
    .run_against_standalone_document(NoAnonymousOperationsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L5>
#[test]
fn valid_l5_named_subscription() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L5",
        super::UPSTREAM_SHA,
    ))
    .code("subscription myData { a }")
    .run_against_standalone_document(NoAnonymousOperationsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L7>
#[test]
fn invalid_l7_anonymous_query() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code("query { a }")
    .errors(vec![ExpectedError::new()
        .message_id("no-anonymous-operations")
        .suggestions(vec![ExpectedSuggestion::new(
            "Rename to `a`",
            "query a { a }",
        )])])
    .run_against_standalone_document(NoAnonymousOperationsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L8>
#[test]
fn invalid_l8_anonymous_mutation_with_alias() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L8",
        super::UPSTREAM_SHA,
    ))
    .code("mutation { renamed: a }")
    .errors(vec![ExpectedError::new()
        .message_id("no-anonymous-operations")
        .suggestions(vec![ExpectedSuggestion::new(
            "Rename to `renamed`",
            "mutation renamed { renamed: a }",
        )])])
    .run_against_standalone_document(NoAnonymousOperationsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L9>
#[test]
fn invalid_l9_anonymous_subscription_with_spread() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-anonymous-operations/index.test.ts#L9",
        super::UPSTREAM_SHA,
    ))
    .code("subscription { ...someFragmentSpread }")
    .errors(vec![ExpectedError::new()
        .message_id("no-anonymous-operations")
        .suggestions(vec![ExpectedSuggestion::new(
            "Rename to `subscription`",
            "subscription subscription { ...someFragmentSpread }",
        )])])
    .run_against_standalone_document(NoAnonymousOperationsRuleImpl);
}
