//! Verbatim port of `@graphql-eslint`'s
//! `require-field-of-type-query-in-mutation-result` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::require_field_of_type_query_in_mutation_result::RequireFieldOfTypeQueryInMutationResultRuleImpl;

/// Upstream uses `useSchema(code)` which appends `type User { id: ID! }` to the
/// schema. We replicate that here so each case sees a complete schema.
fn with_user(code: &str) -> String {
    format!("type User {{\n  id: ID!\n}}\n\n{code}")
}

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L21>
#[test]
fn valid_l21_only_query_type_no_mutation() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L21",
        super::UPSTREAM_SHA,
    ))
    .code(with_user("type Query {\n  user: User\n}"))
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L26>
#[test]
fn valid_l26_mutation_without_query_type() {
    // No `type Query` defined — rule skips because there's no query type to require.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L26",
        super::UPSTREAM_SHA,
    ))
    .code(with_user("# type Query is not defined and no error is reported\ntype Mutation {\n  createUser: User!\n}"))
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L33>
#[test]
fn valid_l33_payload_has_query_field() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .code(with_user(
        "type Query\ntype CreateUserPayload {\n  user: User!\n  query: Query!\n}\n\ntype Mutation {\n  createUser: CreateUserPayload!\n}",
    ))
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L43>
#[test]
fn valid_l43_no_errors_for_union_interface_scalar() {
    // Union, interface, and scalar return types on mutations are skipped.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L43",
        super::UPSTREAM_SHA,
    ))
    .code(with_user(
        "# No errors are reported for type union, interface and scalar\ntype Admin {\n  id: ID!\n}\nunion Union = User | Admin\n\ninterface Interface {\n  id: ID!\n}\n\ntype Query\ntype Mutation {\n  foo: Boolean\n  bar: Union\n  baz: Interface\n}",
    ))
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L62>
#[test]
fn invalid_l62_mutation_result_user_missing_query_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L62",
        super::UPSTREAM_SHA,
    ))
    .code(with_user(
        "type Query\ntype Mutation {\n  createUser(a: User, b: User!, c: [User], d: [User]!, e: [User!]!): User\n}",
    ))
    .errors(vec![
        ExpectedError::new().message(
            "Mutation result type \"User\" must contain field of type \"Query\"",
        ),
    ])
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L71>
#[test]
fn invalid_l71_extend_mutation_result_missing_query() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L71",
        super::UPSTREAM_SHA,
    ))
    .code(with_user(
        "type Query\ntype Mutation\n\nextend type Mutation {\n  createUser: User!\n}",
    ))
    .errors(vec![
        ExpectedError::new().message(
            "Mutation result type \"User\" must contain field of type \"Query\"",
        ),
    ])
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L80>
#[test]
fn invalid_l80_custom_root_types_array_return() {
    // Custom root type names via `schema { … }` block.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L80",
        super::UPSTREAM_SHA,
    ))
    .code(with_user(
        "type RootQuery\ntype RootMutation {\n  createUser: [User]\n}\n\nschema {\n  mutation: RootMutation\n  query: RootQuery\n}",
    ))
    .errors(vec![
        ExpectedError::new().message(
            "Mutation result type \"User\" must contain field of type \"RootQuery\"",
        ),
    ])
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L90>
#[test]
fn invalid_l90_extend_custom_root_mutation() {
    // `extend type RootMutation` with custom root type names.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-field-of-type-query-in-mutation-result/index.test.ts#L90",
        super::UPSTREAM_SHA,
    ))
    .code(with_user(
        "type RootQuery\ntype RootMutation\nextend type RootMutation {\n  createUser: [User!]!\n}\n\nschema {\n  mutation: RootMutation\n  query: RootQuery\n}",
    ))
    .errors(vec![
        ExpectedError::new().message(
            "Mutation result type \"User\" must contain field of type \"RootQuery\"",
        ),
    ])
    .run_against_standalone_schema(RequireFieldOfTypeQueryInMutationResultRuleImpl);
}
