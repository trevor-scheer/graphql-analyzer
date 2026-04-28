//! Verbatim port of `@graphql-eslint`'s `no-deprecated` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts>

use super::harness::{Case, ExpectedError, ExpectedSuggestion};
use crate::rules::no_deprecated::NoDeprecatedRuleImpl;

/// Shared schema used across all upstream test cases.
const TEST_SCHEMA: &str = r#"
  input TestInput {
    a: Int @deprecated(reason: "Use 'b' instead.")
    b: Boolean
  }

  type Query {
    oldField: String @deprecated
    oldFieldWithReason: String @deprecated(reason: "test")
    newField: String!
    testArgument(a: Int @deprecated(reason: "Use 'b' instead."), b: Boolean): Boolean
    testObjectField(input: TestInput): Boolean
  }

  type Mutation {
    something(t: EnumType): Boolean!
  }

  enum EnumType {
    OLD @deprecated
    OLD_WITH_REASON @deprecated(reason: "test")
    NEW
  }
"#;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L39>
#[test]
fn valid_l39_new_field() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L39",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ newField }")
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L40>
#[test]
fn valid_l40_new_enum_value() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L40",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("mutation { something(t: NEW) }")
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L44>
#[test]
fn invalid_l44_deprecated_enum_old() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L44",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("mutation { something(t: OLD) }")
    .errors(vec![ExpectedError::new()
        .message(
            "Enum \"OLD\" is marked as deprecated in your GraphQL schema (reason: No longer supported)",
        )
        // Upstream doesn't pin suggestions; we document ours here.
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove enum \"OLD\"",
            "mutation { something(t: ) }",
        )])])
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L53>
#[test]
fn invalid_l53_deprecated_enum_old_with_reason() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L53",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("mutation { something(t: OLD_WITH_REASON) }")
    .errors(vec![ExpectedError::new()
        .message(
            "Enum \"OLD_WITH_REASON\" is marked as deprecated in your GraphQL schema (reason: test)",
        )
        // Upstream doesn't pin suggestions; we document ours here.
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove enum \"OLD_WITH_REASON\"",
            "mutation { something(t: ) }",
        )])])
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L63>
#[test]
fn invalid_l63_deprecated_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L63",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ oldField }")
    .errors(vec![ExpectedError::new()
        .message(
            "Field \"oldField\" is marked as deprecated in your GraphQL schema (reason: No longer supported)",
        )
        // Upstream doesn't pin suggestions; we document ours here.
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove field \"oldField\"",
            "{  }",
        )])])
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L73>
#[test]
fn invalid_l73_deprecated_field_with_reason() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L73",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ oldFieldWithReason }")
    .errors(vec![ExpectedError::new()
        .message(
            "Field \"oldFieldWithReason\" is marked as deprecated in your GraphQL schema (reason: test)",
        )
        // Upstream doesn't pin suggestions; we document ours here.
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove field \"oldFieldWithReason\"",
            "{  }",
        )])])
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L83>
#[test]
fn invalid_l83_deprecated_argument() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L83",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ testArgument(a: 2) }")
    .errors(vec![ExpectedError::new()
        .message(
            "Argument \"a\" is marked as deprecated in your GraphQL schema (reason: Use 'b' instead.)",
        )
        // Upstream doesn't pin suggestions; we document ours here.
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove argument \"a\"",
            "{ testArgument() }",
        )])])
    .run_against_document_schema(NoDeprecatedRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-deprecated/index.test.ts#L93>
#[test]
fn invalid_l93_deprecated_input_object_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-deprecated/index.test.ts#L93",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ testObjectField(input: { a: 2 }) }")
    .errors(vec![ExpectedError::new()
        .message(
            "Object field \"a\" is marked as deprecated in your GraphQL schema (reason: Use 'b' instead.)",
        )
        // Upstream doesn't pin suggestions; we document ours here.
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove field \"a\"",
            "{ testObjectField(input: {  }) }",
        )])])
    .run_against_document_schema(NoDeprecatedRuleImpl);
}
