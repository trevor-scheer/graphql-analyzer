//! Verbatim port of `@graphql-eslint`'s `no-typename-prefix` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::no_typename_prefix::NoTypenamePrefixRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L6>
#[test]
fn valid_l6_user_type_clean_fields() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L6",
        super::UPSTREAM_SHA,
    ))
    .code("type User {\n  id: ID!\n}")
    .run_against_standalone_schema(NoTypenamePrefixRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L11>
#[test]
fn valid_l11_interface_node_clean_fields() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L11",
        super::UPSTREAM_SHA,
    ))
    .code("interface Node {\n  id: ID!\n}")
    .run_against_standalone_schema(NoTypenamePrefixRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L16>
#[test]
fn valid_l16_eslint_disable_comment() {
    // `# eslint-disable-next-line` suppresses the `userId` diagnostic on the
    // following line, matching upstream's framework-level suppression.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L16",
        super::UPSTREAM_SHA,
    ))
    .code("type User {\n  # eslint-disable-next-line\n  userId: ID!\n}")
    .run_against_standalone_schema(NoTypenamePrefixRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L24>
#[test]
fn invalid_l24_userid_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L24",
        super::UPSTREAM_SHA,
    ))
    .code("type User {\n  userId: ID!\n}")
    .errors(vec![
        ExpectedError::new().message(
            "Field \"userId\" starts with the name of the parent type \"User\"",
        ),
    ])
    .run_against_standalone_schema(NoTypenamePrefixRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L30>
#[test]
fn invalid_l30_userid_and_username_fields() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L30",
        super::UPSTREAM_SHA,
    ))
    .code("type User {\n  userId: ID!\n  userName: String!\n}")
    .errors(vec![
        ExpectedError::new().message(
            "Field \"userId\" starts with the name of the parent type \"User\"",
        ),
        ExpectedError::new().message(
            "Field \"userName\" starts with the name of the parent type \"User\"",
        ),
    ])
    .run_against_standalone_schema(NoTypenamePrefixRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L39>
#[test]
fn invalid_l39_interface_nodeid_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-typename-prefix/index.test.ts#L39",
        super::UPSTREAM_SHA,
    ))
    .code("interface Node {\n  nodeId: ID!\n}")
    .errors(vec![
        ExpectedError::new().message(
            "Field \"nodeId\" starts with the name of the parent type \"Node\"",
        ),
    ])
    .run_against_standalone_schema(NoTypenamePrefixRuleImpl);
}
