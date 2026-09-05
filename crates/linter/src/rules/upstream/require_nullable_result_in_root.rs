//! Verbatim port of `@graphql-eslint`'s `require-nullable-result-in-root` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts>
//!
//! Upstream uses `withSchema({code})` which passes the entire snippet as both
//! the schema and the linted content (a self-contained schema document). We
//! replicate this by passing the full snippet as `code:` to the standalone
//! schema runner.

use super::harness::{Case, ExpectedError};
use crate::rules::require_nullable_result_in_root::RequireNullableResultInRootRuleImpl;

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L7>
#[test]
fn valid_l7_nullable_and_non_null_list_root_fields() {
    // `foo: User` (nullable named), `baz: [User]!` (non-null list — acceptable),
    // `bar: [User!]!` (non-null list — acceptable). None of these trigger the rule.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  foo: User\n  baz: [User]!\n  bar: [User!]!\n}\ntype User {\n  id: ID!\n}",
    )
    .run_against_standalone_schema(RequireNullableResultInRootRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L19>
#[test]
fn invalid_l19_non_null_query_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L19",
        super::UPSTREAM_SHA,
    ))
    .code("type Query {\n  user: User!\n}\ntype User {\n  id: ID!\n}")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireNullableResultInRootRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L27>
#[test]
fn invalid_l27_extend_mutation_non_null() {
    // `extend type MyMutation` with a non-null result on a custom mutation root.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L27",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type MyMutation\nextend type MyMutation {\n  user: User!\n}\ninterface User {\n  id: ID!\n}\nschema {\n  mutation: MyMutation\n}",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireNullableResultInRootRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L40>
#[test]
fn invalid_l40_default_scalar_mutation_field() {
    // A `Mutation` type with a non-null built-in scalar (`Boolean!`).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-result-in-root/index.test.ts#L40",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { foo: Boolean! }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireNullableResultInRootRuleImpl);
}
