//! Verbatim port of `@graphql-eslint`'s `relay-arguments` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-arguments/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::relay_arguments::RelayArgumentsRuleImpl;

/// Upstream wraps each test with this helper that also embeds `PostConnection`
/// and `Query` into the schema. We reproduce the same embedding here.
fn schema_with_types(code: &str) -> String {
    format!(
        "type PostConnection\ntype Query\n{code}",
        code = code.trim()
    )
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-arguments/index.test.ts#L21>
#[test]
fn valid_l21_float_scalar_accepted_for_before() {
    // `before: Float` is valid because `Float` is a built-in scalar type.
    // Upstream notes this with the comment "should be fine as it's Scalar".
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-arguments/index.test.ts#L21",
        super::UPSTREAM_SHA,
    ))
    .code(schema_with_types(
        r#"
        type User {
          posts(
            after: String!
            first: Int!
            before: Float # should be fine as it's Scalar
            last: Int
          ): PostConnection
        }
      "#,
    ))
    .run_against_standalone_schema(RelayArgumentsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-arguments/index.test.ts#L33>
#[test]
fn valid_l33_include_both_false_forward_or_backward_ok() {
    // With `includeBoth: false`, either forward or backward pagination alone
    // is sufficient.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-arguments/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .code(schema_with_types(
        r#"
        type User {
          posts(after: String!, first: Int!): PostConnection
          comments(before: Float, last: Int): PostConnection
        }
      "#,
    ))
    .options(serde_json::json!({ "includeBoth": false }))
    .run_against_standalone_schema(RelayArgumentsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-arguments/index.test.ts#L45>
#[test]
fn invalid_l45_missing_args_and_wrong_types() {
    // `posts` has no pagination args → 1 MISSING_ARGUMENTS error.
    // `comments` has 4 args all wrong:
    //   - after: [String!]!  → list, not allowed → 1 error
    //   - first: Float       → must be Int → 1 error
    //   - before: Query      → Query is not String or Scalar → 1 error
    //   - last: [PostConnection] → list, not allowed → 1 error
    // Total: 5 errors. Upstream uses `errors: 5` (count only, no message pins).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-arguments/index.test.ts#L45",
        super::UPSTREAM_SHA,
    ))
    .code(schema_with_types(
        r#"
        type User {
          posts: PostConnection
          comments(
            after: [String!]!
            first: Float
            before: Query
            last: [PostConnection]
          ): PostConnection
        }
      "#,
    ))
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RelayArgumentsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-arguments/index.test.ts#L64>
#[test]
fn invalid_l64_missing_backward_args_with_forward_present() {
    // Forward pair present (`after` + `first`) but no backward pair → 2 errors.
    // Upstream uses `errors: 2` (count only, no message pins).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-arguments/index.test.ts#L64",
        super::UPSTREAM_SHA,
    ))
    .code(schema_with_types(
        r#"
        type User {
          posts(after: String, first: Int): PostConnection
        }
      "#,
    ))
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(RelayArgumentsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-arguments/index.test.ts#L74>
#[test]
fn invalid_l74_include_both_false_missing_paired_arg() {
    // `includeBoth: false`, `posts(after: String, first: Int, before: Float)`:
    // forward pair is complete; `before` is present so backward pair fires
    // and `last` is missing → 1 error.
    // Upstream uses `errors: 1` (count only, no message pins).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-arguments/index.test.ts#L74",
        super::UPSTREAM_SHA,
    ))
    .code(schema_with_types(
        r#"
        type User {
          posts(after: String, first: Int, before: Float): PostConnection
        }
      "#,
    ))
    .options(serde_json::json!({ "includeBoth": false }))
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayArgumentsRuleImpl);
}
