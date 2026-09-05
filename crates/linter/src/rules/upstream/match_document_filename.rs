//! Verbatim port of `@graphql-eslint`'s `match-document-filename` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::match_document_filename::MatchDocumentFilenameRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L7>
#[test]
fn valid_l7_gql_extension_match() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .filename("src/me.gql")
    .code("{ me }")
    .options(serde_json::json!({ "fileExtension": ".gql" }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L12>
#[test]
fn valid_l12_kebab_case_query_with_suffix() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L12",
        super::UPSTREAM_SHA,
    ))
    .filename("src/user-by-id.query.gql")
    .code("query USER_BY_ID { user { id } }")
    .options(serde_json::json!({ "query": { "style": "kebab-case", "suffix": ".query" } }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L17>
#[test]
fn valid_l17_camel_case_mutation_with_query_suffix() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L17",
        super::UPSTREAM_SHA,
    ))
    .filename("src/createUserQuery.gql")
    .code("mutation CREATE_USER { user { id } }")
    .options(serde_json::json!({ "mutation": { "style": "camelCase", "suffix": "Query" } }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L22>
#[test]
fn valid_l22_upper_case_subscription() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L22",
        super::UPSTREAM_SHA,
    ))
    .filename("src/NEW_USER.gql")
    .code("subscription new_user { user { id } }")
    .options(serde_json::json!({ "subscription": { "style": "UPPER_CASE" } }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L27>
#[test]
fn valid_l27_snake_case_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L27",
        super::UPSTREAM_SHA,
    ))
    .filename("src/user_fields.gql")
    .code("fragment UserFields on User { id }")
    .options(serde_json::json!({ "fragment": { "style": "snake_case" } }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L32>
#[test]
fn valid_l32_pascal_case_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L32",
        super::UPSTREAM_SHA,
    ))
    .filename("src/UserById.gql")
    .code("query USER_BY_ID { user { id } }")
    .options(serde_json::json!({ "query": { "style": "PascalCase" } }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L37>
#[test]
fn valid_l37_match_document_style_string_shorthand() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L37",
        super::UPSTREAM_SHA,
    ))
    .filename("src/SAMEAsOperation.gql")
    .code("query SAMEAsOperation { foo }")
    .options(serde_json::json!({ "query": "matchDocumentStyle" }))
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L44>
#[test]
fn invalid_l44_graphql_extension_mismatch() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L44",
        super::UPSTREAM_SHA,
    ))
    .filename("src/queries/me.graphql")
    .code("{ me }")
    .options(serde_json::json!({ "fileExtension": ".gql" }))
    .errors(vec![
        ExpectedError::new()
            .message("File extension \".graphql\" don't match extension \".gql\""),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L50>
#[test]
fn invalid_l50_kebab_filename_not_pascal_query() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L50",
        super::UPSTREAM_SHA,
    ))
    .filename("src/user-by-id.gql")
    .code("query UserById { user { id } }")
    .options(serde_json::json!({ "query": { "style": "PascalCase" } }))
    .errors(vec![
        ExpectedError::new()
            .message("Unexpected filename \"user-by-id.gql\". Rename it to \"UserById.gql\""),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L56>
#[test]
fn invalid_l56_camel_filename_not_pascal_with_suffix() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L56",
        super::UPSTREAM_SHA,
    ))
    .filename("src/userById.gql")
    .code("query UserById { user { id } }")
    .options(serde_json::json!({ "query": { "style": "PascalCase", "suffix": ".query" } }))
    .errors(vec![
        ExpectedError::new()
            .message("Unexpected filename \"userById.gql\". Rename it to \"UserById.query.gql\""),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L63>
#[test]
fn invalid_l63_kebab_fragment_not_pascal() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L63",
        super::UPSTREAM_SHA,
    ))
    .filename("src/user-fields.gql")
    .code("fragment UserFields on User { id }")
    .options(serde_json::json!({ "fragment": { "style": "PascalCase" } }))
    .errors(vec![
        ExpectedError::new()
            .message("Unexpected filename \"user-fields.gql\". Rename it to \"UserFields.gql\""),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L70>
#[test]
fn invalid_l70_first_operation_only() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L70",
        super::UPSTREAM_SHA,
    ))
    .filename("src/getUsersQuery.gql")
    .code("query getUsers { users } mutation createPost { createPost }")
    .options(serde_json::json!({
        "query": { "style": "PascalCase", "suffix": ".query" },
        "mutation": { "style": "PascalCase", "suffix": ".mutation" },
    }))
    .errors(vec![
        ExpectedError::new()
            .message("Unexpected filename \"getUsersQuery.gql\". Rename it to \"GetUsers.query.gql\""),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L83>
#[test]
fn invalid_l83_first_operation_over_fragment() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L83",
        super::UPSTREAM_SHA,
    ))
    .filename("src/getUsersQuery.gql")
    .code("
        fragment UserFields on User {
          id
        }
        query getUsers {
          users {
            ...UserFields
          }
        }
      ")
    .options(serde_json::json!({
        "query": { "style": "PascalCase", "suffix": ".query" },
        "fragment": { "style": "PascalCase", "suffix": ".fragment" },
    }))
    .errors(vec![
        ExpectedError::new()
            .message("Unexpected filename \"getUsersQuery.gql\". Rename it to \"GetUsers.query.gql\""),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/match-document-filename/index.test.ts#L107>
#[test]
fn invalid_l107_mutation_with_prefix() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/match-document-filename/index.test.ts#L107",
        super::UPSTREAM_SHA,
    ))
    .filename("add-alert-channel.graphql")
    .code("
        mutation addAlertChannel {
          foo
        }
      ")
    .options(serde_json::json!({ "mutation": { "prefix": "mutation." } }))
    .errors(vec![
        ExpectedError::new().message(
            "Unexpected filename \"add-alert-channel.graphql\". Rename it to \"mutation.add-alert-channel.graphql\""
        ),
    ])
    .run_against_standalone_document(MatchDocumentFilenameRuleImpl);
}
