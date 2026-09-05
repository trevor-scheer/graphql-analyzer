//! Verbatim port of `@graphql-eslint`'s `no-scalar-result-type-on-mutation` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts>
//!
//! Upstream wraps each `code:` inside a schema that also includes `type User { id: ID! }`.
//! Our harness takes `code:` as the entire schema file, so we inline that base type
//! wherever the rule needs it to parse correctly. Cases that don't reference `User`
//! at all don't need it.

use super::harness::{Case, ExpectedError};
use crate::rules::no_scalar_result_type_on_mutation::NoScalarResultTypeOnMutationRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L22>
#[test]
fn valid_l22_query_type_with_boolean() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L22",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype Query {\n  good: Boolean\n}",
    )
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L27>
#[test]
fn valid_l27_mutation_returns_object() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L27",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype Mutation {\n  createUser: User!\n}",
    )
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L33>
#[test]
fn valid_l33_root_mutation_via_schema_directive() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype RootMutation {\n  createUser: User!\n}\nschema {\n  mutation: RootMutation\n}",
    )
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L43>
#[test]
fn invalid_l43_should_ignore_arguments() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L43",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype Mutation {\n  createUser(a: ID, b: ID!, c: [ID]!, d: [ID!]!): Boolean\n}",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L51>
#[test]
fn invalid_l51_extend_mutation_returns_boolean() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L51",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype Mutation\nextend type Mutation {\n  createUser: Boolean!\n}",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L58>
#[test]
fn invalid_l58_root_mutation_via_schema_returns_list_boolean() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L58",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype RootMutation {\n  createUser: [Boolean]\n}\nschema {\n  mutation: RootMutation\n}",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L66>
#[test]
fn invalid_l66_extend_root_mutation_returns_list_boolean() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L66",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype RootMutation\nextend type RootMutation {\n  createUser: [Boolean]!\n}\nschema {\n  mutation: RootMutation\n}",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L74>
#[test]
fn invalid_l74_multiple_scalar_fields() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-scalar-result-type-on-mutation/index.test.ts#L74",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User { id: ID! }\ntype Mutation {\n  createUser: User!\n  updateUser: Int\n  deleteUser: [Boolean!]!\n}",
    )
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(NoScalarResultTypeOnMutationRuleImpl);
}
