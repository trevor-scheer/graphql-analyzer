//! Verbatim port of `@graphql-eslint`'s `input-name` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts>

use super::harness::{Case, ExpectedError, ExpectedSuggestion};
use crate::rules::input_name::InputNameRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L7>
#[test]
fn valid_l7_set_message_with_input_type_check() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { SetMessage(input: SetMessageInput): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L11>
#[test]
fn valid_l11_two_mutations_with_input_type_check() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L11",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { CreateMessage(input: CreateMessageInput): String DeleteMessage(input: DeleteMessageInput): Boolean }")
    .options(serde_json::json!({ "checkInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L15>
#[test]
fn valid_l15_nonnull_input_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L15",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { CreateMessage(input: CreateMessageInput!): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L19>
#[test]
fn valid_l19_list_input_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L19",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { CreateMessage(input: [CreateMessageInput]): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L22>
#[test]
fn valid_l22_scalar_input_no_type_check() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L22",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { CreateMessage(input: String): String }")
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L23>
#[test]
fn valid_l23_extend_mutation_scalar_input() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L23",
        super::UPSTREAM_SHA,
    ))
    .code("extend type Mutation { CreateMessage(input: String): String }")
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L24>
#[test]
fn valid_l24_query_not_checked_by_default() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L24",
        super::UPSTREAM_SHA,
    ))
    .code("type Query { message(id: ID): Message }")
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L25>
#[test]
fn valid_l25_extend_query_not_checked_by_default() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L25",
        super::UPSTREAM_SHA,
    ))
    .code("extend type Query { message(id: ID): Message }")
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L27>
#[test]
fn valid_l27_case_insensitive_input_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L27",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(input: UserCreateInput): String }")
    .options(serde_json::json!({ "checkInputType": true, "caseSensitiveInputType": false }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L31>
#[test]
fn valid_l31_case_sensitive_lowercase_input_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L31",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(input: userCreateInput): String }")
    .options(serde_json::json!({ "checkInputType": true, "caseSensitiveInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L35>
#[test]
fn valid_l35_check_mutations_false_nonconforming_input_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L35",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { SetMessage(input: NonConforming): String }")
    .options(serde_json::json!({ "checkMutations": false, "checkInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L39>
#[test]
fn valid_l39_check_mutations_true_no_type_check() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L39",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { SetMessage(input: String): String }")
    .options(serde_json::json!({ "checkMutations": true, "checkInputType": false }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L43>
#[test]
fn valid_l43_check_queries_false_nonconforming_input_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L43",
        super::UPSTREAM_SHA,
    ))
    .code("type Query { getMessage(input: NonConforming): String }")
    .options(serde_json::json!({ "checkQueries": false, "checkInputType": true }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L47>
#[test]
fn valid_l47_check_queries_true_no_type_check() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L47",
        super::UPSTREAM_SHA,
    ))
    .code("type Query { getMessage(input: String): String }")
    .options(serde_json::json!({ "checkQueries": true, "checkInputType": false }))
    .run_against_standalone_schema(InputNameRuleImpl);
}

// Helper to build an arg-name error expectation (1 "Rename to `input`" suggestion).
fn arg_name_error() -> ExpectedError {
    // Upstream doesn't pin messageId or suggestions; we document ours here.
    ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Rename to `input`", "")])
}

// Helper to build an input-type error expectation (1 "Rename to `{...}`" suggestion).
fn input_type_error(expected_name: &str) -> ExpectedError {
    // Upstream doesn't pin messageId or suggestions; we document ours here.
    ExpectedError::new().suggestions(vec![ExpectedSuggestion::new(
        format!("Rename to `{expected_name}`"),
        "",
    )])
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L53>
#[test]
fn invalid_l53_set_message_wrong_name_and_type() {
    // Upstream: errors: 2 (arg name `message` wrong + type `String` wrong).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L53",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { SetMessage(message: String): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![
        arg_name_error(),
        input_type_error("SetMessageInput"),
    ])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L58>
#[test]
fn invalid_l58_set_message_wrong_input_type_only() {
    // Upstream: errors: 1 (arg name is `input`, only the type is wrong).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L58",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { SetMessage(input: String): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![input_type_error("SetMessageInput")])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L63>
#[test]
fn invalid_l63_set_message_wrong_arg_name_only() {
    // Upstream: errors: 1 (type `SetMessageInput` is matching, only the arg name is wrong).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L63",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { SetMessage(hello: SetMessageInput): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![arg_name_error()])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L68>
#[test]
fn invalid_l68_user_create_nonnull_input_type() {
    // Upstream: errors: 2 (arg name `record` wrong + type `CreateOneUserInput` wrong).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L68",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(record: CreateOneUserInput!): CreateOneUserPayload }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![
        arg_name_error(),
        input_type_error("userCreateInput"),
    ])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L73>
#[test]
fn invalid_l73_user_create_list_nonnull_type() {
    // Upstream: errors: 2 (arg name + type wrong; list/nonnull wrappers ignored
    // when looking up the inner type name).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L73",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(record: [CreateOneUserInput]!): CreateOneUserPayload }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![
        arg_name_error(),
        input_type_error("userCreateInput"),
    ])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L78>
#[test]
fn invalid_l78_user_create_list_nonnull_inner_and_outer() {
    // Upstream: errors: 2.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L78",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(record: [CreateOneUserInput!]!): CreateOneUserPayload }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![
        arg_name_error(),
        input_type_error("userCreateInput"),
    ])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L83>
#[test]
fn invalid_l83_user_create_list_nonnull_inner_only() {
    // Upstream: errors: 2.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L83",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(record: [CreateOneUserInput!]): CreateOneUserPayload }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![
        arg_name_error(),
        input_type_error("userCreateInput"),
    ])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L88>
#[test]
fn invalid_l88_two_wrong_args_with_type_check() {
    // Upstream: errors: 4 (2 arg names wrong + 2 input types wrong).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L88",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(record: String, test: String): String }")
    .options(serde_json::json!({ "checkInputType": true }))
    .errors(vec![
        // Errors are emitted per-argument: name check then type check for each arg.
        arg_name_error(),
        input_type_error("userCreateInput"),
        arg_name_error(),
        input_type_error("userCreateInput"),
    ])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L93>
#[test]
fn invalid_l93_two_wrong_args_no_type_check() {
    // Upstream: errors: 2 (only arg names checked when `checkInputType: false`).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L93",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(record: String, test: String): String }")
    .options(serde_json::json!({ "checkInputType": false }))
    .errors(vec![arg_name_error(), arg_name_error()])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L98>
#[test]
fn invalid_l98_case_insensitive_type_check_scalar_mismatch() {
    // Upstream: errors: 1 (type `String` doesn't match `userCreateInput` case-insensitively).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L98",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(input: String): String }")
    .options(serde_json::json!({ "checkInputType": true, "caseSensitiveInputType": false }))
    .errors(vec![input_type_error("userCreateInput")])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L103>
#[test]
fn invalid_l103_case_sensitive_type_mismatch() {
    // Upstream: errors: 1 (type is `UserCreateInput` but case-sensitive rule expects
    // `userCreateInput`).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L103",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { userCreate(input: UserCreateInput): String }")
    .options(serde_json::json!({ "checkInputType": true, "caseSensitiveInputType": true }))
    .errors(vec![input_type_error("userCreateInput")])
    .run_against_standalone_schema(InputNameRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/input-name/index.test.ts#L108>
#[test]
fn invalid_l108_query_check_with_wrong_type() {
    // Upstream: errors: 1 (checkQueries enabled; type `GetUserInput` wrong for
    // case-sensitive expected `getUserInput`).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/input-name/index.test.ts#L108",
        super::UPSTREAM_SHA,
    ))
    .code("type Query { getUser(input: GetUserInput): String }")
    .options(serde_json::json!({
        "checkQueries": true,
        "checkInputType": true,
        "caseSensitiveInputType": true,
    }))
    .errors(vec![input_type_error("getUserInput")])
    .run_against_standalone_schema(InputNameRuleImpl);
}
