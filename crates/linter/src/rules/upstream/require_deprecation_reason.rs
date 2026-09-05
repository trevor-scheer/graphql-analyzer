//! Verbatim port of `@graphql-eslint`'s `require-deprecation-reason` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-reason/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::require_deprecation_reason::RequireDeprecationReasonRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-reason/index.test.ts#L6>
#[test]
fn valid_l6_query_no_schema_types() {
    // Upstream valid[0]: a bare operation — no @deprecated anywhere.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-reason/index.test.ts#L6",
        super::UPSTREAM_SHA,
    ))
    .code("query getUser { f a b }")
    .run_against_standalone_schema(RequireDeprecationReasonRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-reason/index.test.ts#L12>
#[test]
fn valid_l12_deprecated_with_various_reason_types() {
    // Upstream valid[1]: @deprecated(reason: "Reason"), @deprecated(reason: 0),
    // @deprecated(reason: 1.5) — all have a reason argument so no errors.
    // (Numeric reasons are unusual but the rule only checks presence, not type.)
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-reason/index.test.ts#L12",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type test {
          field1: String @authorized
          field2: Number
          field4: String @deprecated(reason: "Reason")
        }

        enum testEnum {
          item1 @authorized
          item2 @deprecated(reason: 0)
          item3
        }

        interface testInterface {
          field1: String @authorized
          field2: Number
          field3: String @deprecated(reason: 1.5)
        }
        "#,
    )
    .run_against_standalone_schema(RequireDeprecationReasonRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-reason/index.test.ts#L28>
#[test]
fn invalid_l28_mixed_deprecated_fields() {
    // Upstream expects 7 errors:
    //   1. `deprecatedWithoutReason` field — no reason
    //   2. `item1` enum value — no reason
    //   3. `item1` interface field — no reason
    //   4. `item4` interface field — reason: "" (empty string)
    //   5. `item5` interface field — reason: "  " (whitespace-only)
    //   6. `type MyQuery @deprecated` — type-level @deprecated without reason
    //   7. `foo` input field — no reason
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-reason/index.test.ts#L28",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type A {
          deprecatedWithoutReason: String @deprecated
          deprecatedWithReason: String @deprecated(reason: "Reason")
          notDeprecated: String
        }

        enum TestEnum {
          item1 @deprecated
          item2 @deprecated(reason: "Reason")
        }

        interface TestInterface {
          item1: String @deprecated
          item2: Number @deprecated(reason: "Reason")
          item3: String
          item4: String @deprecated(reason: "")
          item5: String @deprecated(reason: "  ")
        }

        type MyQuery @deprecated

        input MyInput {
          foo: String! @deprecated
        }
        "#,
    )
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RequireDeprecationReasonRuleImpl);
}
