//! Verbatim port of `@graphql-eslint`'s `require-type-pattern-with-oneof` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::require_type_pattern_with_oneof::RequireTypePatternWithOneofRuleImpl;

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L5>
#[test]
fn valid_l5_type_with_ok_and_error_fields() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L5",
        super::UPSTREAM_SHA,
    ))
    .code("type T @oneOf {\n  ok: Ok\n  error: Error\n}")
    .run_against_standalone_schema(RequireTypePatternWithOneofRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L11>
#[test]
fn valid_l11_type_without_oneof_ignored() {
    // Types without `@oneOf` are not subject to the rule.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L11",
        super::UPSTREAM_SHA,
    ))
    .code("type T {\n  notok: Ok\n  err: Error\n}")
    .run_against_standalone_schema(RequireTypePatternWithOneofRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L18>
#[test]
fn valid_l18_input_with_oneof_ignored() {
    // `input` types with `@oneOf` are not checked by this rule — it only
    // validates output `type` definitions.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L18",
        super::UPSTREAM_SHA,
    ))
    .code("input I {\n  notok: Ok\n  err: Error\n}")
    .run_against_standalone_schema(RequireTypePatternWithOneofRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L27>
#[test]
fn invalid_l27_missing_ok_field() {
    // `@oneOf` type has `error` but the field is named `notok` instead of `ok`.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L27",
        super::UPSTREAM_SHA,
    ))
    .code("type T @oneOf {\n  notok: Ok\n  error: Error\n}")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireTypePatternWithOneofRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L36>
#[test]
fn invalid_l36_missing_error_field() {
    // `@oneOf` type has `ok` but the field is named `err` instead of `error`.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-type-pattern-with-oneof/index.test.ts#L36",
        super::UPSTREAM_SHA,
    ))
    .code("type T @oneOf {\n  ok: Ok\n  err: Error\n}")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireTypePatternWithOneofRuleImpl);
}
