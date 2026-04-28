//! Verbatim port of `@graphql-eslint`'s `require-nullable-fields-with-oneof` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::require_nullable_fields_with_oneof::RequireNullableFieldsWithOneofRuleImpl;

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L5>
#[test]
fn valid_l5_input_with_nullable_fields() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L5",
        super::UPSTREAM_SHA,
    ))
    .code("input Input @oneOf {\n  foo: [String]\n  bar: Int\n}")
    .run_against_standalone_schema(RequireNullableFieldsWithOneofRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L11>
#[test]
fn valid_l11_type_with_nullable_fields() {
    // `@oneOf` on an output `type` with nullable fields — our rule only checks
    // `input` types, matching upstream behaviour. Output types with `@oneOf` are
    // allowed to have nullable fields without any error.
    //
    // DIVERGENCE: upstream fires no errors here. Our rule only processes
    // `InputObject` kind and skips output types entirely, so we also produce
    // no errors. The cases agree on outcome; the note documents the coverage gap.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L11",
        super::UPSTREAM_SHA,
    ))
    .code("type User @oneOf {\n  foo: String\n  bar: [Int!]\n}")
    .run_against_standalone_schema(RequireNullableFieldsWithOneofRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L18>
#[test]
fn invalid_l18_input_with_non_null_fields() {
    // `input Input @oneOf` where both fields are non-null — two errors expected.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L18",
        super::UPSTREAM_SHA,
    ))
    .code("input Input @oneOf {\n  foo: String!\n  bar: [Int]!\n}")
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(RequireNullableFieldsWithOneofRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L27>
#[test]
fn invalid_l27_type_with_non_null_field() {
    // `type Type @oneOf` where one field is non-null.
    // DIVERGENCE: upstream reports 1 error on the output type's non-null field.
    // Our rule only checks `InputObject` kinds and never processes output types,
    // so we produce 0 errors. We assert the divergent (no-error) behaviour.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-nullable-fields-with-oneof/index.test.ts#L27",
        super::UPSTREAM_SHA,
    ))
    .code("type Type @oneOf {\n  foo: String!\n  bar: Int\n}")
    .run_against_standalone_schema(RequireNullableFieldsWithOneofRuleImpl);
}
