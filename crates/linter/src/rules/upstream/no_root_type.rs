//! Verbatim port of `@graphql-eslint`'s `no-root-type` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-root-type/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::no_root_type::NoRootTypeRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-root-type/index.test.ts#L18>
#[test]
fn valid_l18_query_with_disallow_mutation_subscription() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-root-type/index.test.ts#L18",
        super::UPSTREAM_SHA,
    ))
    .code("type Query")
    .options(serde_json::json!({ "disallow": ["mutation", "subscription"] }))
    .run_against_standalone_schema(NoRootTypeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-root-type/index.test.ts#L24>
#[test]
fn invalid_l24_disallow_mutation() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-root-type/index.test.ts#L24",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation")
    .options(serde_json::json!({ "disallow": ["mutation"] }))
    .errors(vec![
        ExpectedError::new().message("Root type `Mutation` is forbidden."),
    ])
    .run_against_standalone_schema(NoRootTypeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-root-type/index.test.ts#L30>
#[test]
fn invalid_l30_disallow_subscription() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-root-type/index.test.ts#L30",
        super::UPSTREAM_SHA,
    ))
    .code("type Subscription")
    .options(serde_json::json!({ "disallow": ["subscription"] }))
    .errors(vec![
        ExpectedError::new().message("Root type `Subscription` is forbidden."),
    ])
    .run_against_standalone_schema(NoRootTypeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-root-type/index.test.ts#L36>
#[test]
fn invalid_l36_disallow_with_extend() {
    // Upstream: `code = 'extend type Mutation { foo: ID }'` with
    // `schema = 'type Mutation'` prepended. We place the combined SDL as the
    // schema-rule code so that the schema parser sees both nodes.
    //
    // DIVERGENCE: upstream puts base `type Mutation` in the shared schema and
    // `extend type Mutation { foo: ID }` as the linted `code:`, then flags the
    // *extension* node. Our schema loader merges all SDL into one view and our
    // rule flags the base type definition, not the extension.  We therefore
    // provide both in `code:` and assert the error message only.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-root-type/index.test.ts#L36",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation\nextend type Mutation { foo: ID }")
    .options(serde_json::json!({ "disallow": ["mutation"] }))
    .errors(vec![
        ExpectedError::new().message("Root type `Mutation` is forbidden."),
    ])
    .run_against_standalone_schema(NoRootTypeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-root-type/index.test.ts#L43>
#[test]
fn invalid_l43_disallow_when_root_type_name_is_renamed() {
    // Upstream: `code = 'type MyMutation'`, schema = `'schema { mutation: MyMutation }'` prepended.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-root-type/index.test.ts#L43",
        super::UPSTREAM_SHA,
    ))
    .code("schema { mutation: MyMutation }\ntype MyMutation")
    .options(serde_json::json!({ "disallow": ["mutation"] }))
    .errors(vec![
        ExpectedError::new().message("Root type `MyMutation` is forbidden."),
    ])
    .run_against_standalone_schema(NoRootTypeRuleImpl);
}
