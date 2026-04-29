//! Verbatim port of `@graphql-eslint`'s `unique-enum-value-names` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/unique-enum-value-names/index.test.ts>
//!
//! Upstream has no valid cases (`valid: []`).

use super::harness::{Case, ExpectedError};
use crate::rules::unique_enum_value_names::UniqueEnumValueNamesRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/unique-enum-value-names/index.test.ts#L7>
#[test]
fn invalid_l7_case_insensitive_duplicate_in_enum() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/unique-enum-value-names/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code("enum A { TEST TesT }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(UniqueEnumValueNamesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/unique-enum-value-names/index.test.ts#L11>
#[test]
fn invalid_l11_case_insensitive_duplicate_in_enum_extension() {
    // `extend enum` is lowered to the same HIR `TypeDef` as a base enum
    // definition; the duplicate-value check applies identically.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/unique-enum-value-names/index.test.ts#L11",
        super::UPSTREAM_SHA,
    ))
    .code("extend enum A { TEST TesT }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(UniqueEnumValueNamesRuleImpl);
}
