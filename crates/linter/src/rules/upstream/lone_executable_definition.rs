//! Verbatim port of `@graphql-eslint`'s `lone-executable-definition` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::lone_executable_definition::LoneExecutableDefinitionRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L7>
#[test]
fn valid_l7_single_shorthand_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code("
        {
          id
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L14>
#[test]
fn valid_l14_single_anonymous_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L14",
        super::UPSTREAM_SHA,
    ))
    .code("
        query {
          id
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L22>
#[test]
fn valid_l22_single_named_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L22",
        super::UPSTREAM_SHA,
    ))
    .code("
        query Foo {
          id
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L30>
#[test]
fn valid_l30_single_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L30",
        super::UPSTREAM_SHA,
    ))
    .code("
        fragment Foo on Bar {
          id
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L38>
#[test]
fn valid_l38_single_anonymous_mutation() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L38",
        super::UPSTREAM_SHA,
    ))
    .code("
        mutation ($name: String!) {
          createFoo {
            name
          }
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L48>
#[test]
fn valid_l48_single_named_mutation() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L48",
        super::UPSTREAM_SHA,
    ))
    .code("
        mutation Foo($name: String!) {
          createFoo {
            name
          }
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L58>
#[test]
fn valid_l58_single_anonymous_subscription() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L58",
        super::UPSTREAM_SHA,
    ))
    .code("
        subscription {
          id
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L66>
#[test]
fn valid_l66_single_named_subscription() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L66",
        super::UPSTREAM_SHA,
    ))
    .code("
        subscription Foo {
          id
        }
      ")
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L75>
#[test]
fn valid_l75_fragments_ignored_alongside_shorthand_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L75",
        super::UPSTREAM_SHA,
    ))
    .code("
        {
          id
        }
        fragment Bar on Bar {
          id
        }
      ")
    .options(serde_json::json!({ "ignore": ["fragment"] }))
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L87>
#[test]
fn valid_l87_fragment_first_then_query_with_ignore() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L87",
        super::UPSTREAM_SHA,
    ))
    .code("
        fragment Bar on Bar {
          id
        }
        query Foo {
          id
        }
      ")
    .options(serde_json::json!({ "ignore": ["fragment"] }))
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L101>
#[test]
fn invalid_l101_report_additional_definitions() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L101",
        super::UPSTREAM_SHA,
    ))
    .code("
        query Valid {
          id
        }
        {
          id
        }
        fragment Bar on Bar {
          id
        }
        mutation ($name: String!) {
          createFoo {
            name
          }
        }
        mutation Baz($name: String!) {
          createFoo {
            name
          }
        }
        subscription {
          id
        }
        subscription Sub {
          id
        }
      ")
    .errors(vec![
        ExpectedError::new().message("Query should be in a separate file."),
        ExpectedError::new().message("Fragment \"Bar\" should be in a separate file."),
        ExpectedError::new().message("Mutation should be in a separate file."),
        ExpectedError::new().message("Mutation \"Baz\" should be in a separate file."),
        ExpectedError::new().message("Subscription should be in a separate file."),
        ExpectedError::new().message("Subscription \"Sub\" should be in a separate file."),
    ])
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L138>
#[test]
fn invalid_l138_report_definitions_after_shorthand_query() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L138",
        super::UPSTREAM_SHA,
    ))
    .code("
        {
          id
        }
        fragment Bar on Bar {
          id
        }
      ")
    .errors(vec![
        ExpectedError::new().message("Fragment \"Bar\" should be in a separate file."),
    ])
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L150>
#[test]
fn invalid_l150_ignore_fragment_but_report_mutation() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/lone-executable-definition/index.test.ts#L150",
        super::UPSTREAM_SHA,
    ))
    .code("
        query Foo {
          id
        }
        fragment Bar on Bar {
          id
        }
        mutation Baz($name: String!) {
          createFoo {
            name
          }
        }
      ")
    .options(serde_json::json!({ "ignore": ["fragment"] }))
    .errors(vec![
        ExpectedError::new().message("Mutation \"Baz\" should be in a separate file."),
    ])
    .run_against_standalone_document(LoneExecutableDefinitionRuleImpl);
}
