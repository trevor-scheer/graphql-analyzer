//! Verbatim port of `@graphql-eslint`'s `no-duplicate-fields` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-duplicate-fields/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::no_duplicate_fields::NoDuplicateFieldsRuleImpl;

// ---------------------------------------------------------------------------
// valid
// ---------------------------------------------------------------------------

// Upstream has no valid cases for this rule.

// ---------------------------------------------------------------------------
// invalid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L9>
#[test]
fn invalid_l9_duplicate_variable() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L9",
        super::UPSTREAM_SHA,
    ))
    .code(
        "query test($v: String, $t: String, $v: String) {\n  id\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message("Variable `v` defined multiple times."),
    ])
    .run_against_standalone_document(NoDuplicateFieldsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L15>
#[test]
fn invalid_l15_duplicate_argument() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L15",
        super::UPSTREAM_SHA,
    ))
    .code(
        "query test {\n  users(first: 100, after: 10, filter: \"test\", first: 50) {\n    id\n  }\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message("Argument `first` defined multiple times."),
    ])
    .run_against_standalone_document(NoDuplicateFieldsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L23>
#[test]
fn invalid_l23_duplicate_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L23",
        super::UPSTREAM_SHA,
    ))
    .code(
        "query test {\n  users {\n    id\n    name\n    email\n    name\n  }\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message("Field `name` defined multiple times."),
    ])
    .run_against_standalone_document(NoDuplicateFieldsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L33>
/// Alias reuse: `email: somethingElse` duplicates the `email` response name.
#[test]
fn invalid_l33_duplicate_alias_response_name() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-duplicate-fields/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .code(
        "query test {\n  users {\n    id\n    name\n    email\n    email: somethingElse\n  }\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message("Field `email` defined multiple times."),
    ])
    .run_against_standalone_document(NoDuplicateFieldsRuleImpl);
}
