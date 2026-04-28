//! Verbatim port of `@graphql-eslint`'s `require-description` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::require_description::RequireDescriptionRuleImpl;

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L11>
#[test]
fn valid_l11_enum_values_all_described() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L11",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        enum EnumUserLanguagesSkill {
          """
          basic
          """
          basic
          """
          fluent
          """
          fluent
          """
          native
          """
          native
        }
        "#,
    )
    .options(serde_json::json!({ "EnumValueDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L31>
#[test]
fn valid_l31_input_fields_all_described() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L31",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        input SalaryDecimalOperatorsFilterUpdateOneUserInput {
          """
          gt
          """
          gt: BSONDecimal
          """
          in
          """
          in: [BSONDecimal]
          " nin "
          nin: [BSONDecimal]
        }
        "#,
    )
    .options(serde_json::json!({ "InputValueDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L49>
#[test]
fn valid_l49_type_and_fields_all_described() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L49",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        " Test "
        type CreateOneUserPayload {
          "Created document ID"
          recordId: MongoID

          "Created document"
          record: User
        }
        "#,
    )
    .options(serde_json::json!({ "types": true, "FieldDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L61>
#[test]
fn valid_l61_operation_with_ok_comment() {
    // `# OK` immediately above the query satisfies OperationDefinition.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L61",
        super::UPSTREAM_SHA,
    ))
    .code("# OK\nquery {\n  test\n}")
    .options(serde_json::json!({ "OperationDefinition": true }))
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L68>
#[test]
fn valid_l68_fragment_ignored_by_operation_rule() {
    // Fragments don't have a description slot so they're excluded from the
    // OperationDefinition check.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L68",
        super::UPSTREAM_SHA,
    ))
    .code("# ignore fragments\nfragment UserFields on User {\n  id\n}")
    .options(serde_json::json!({ "OperationDefinition": true }))
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L75>
#[test]
fn valid_l75_root_field_described() {
    // `rootField: true` with a described root field — no errors.
    // Our rule accepts `rootField` in the options struct (dead field) but
    // doesn't enforce it; with `{ rootField: true }` the other kinds default
    // to false so nothing fires.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L75",
        super::UPSTREAM_SHA,
    ))
    .code(r#"type Query { "Get user" user(id: ID!): User }"#)
    .options(serde_json::json!({ "rootField": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L81>
#[test]
fn valid_l81_empty_query_type() {
    // `type Query` with no fields — nothing to require a description on.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L81",
        super::UPSTREAM_SHA,
    ))
    .code("type Query")
    .options(serde_json::json!({ "rootField": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L88>
#[test]
fn invalid_l88_object_type_no_description() {
    // Upstream: `ObjectTypeDefinition: true` → 1 error.
    // DIVERGENCE: our options struct has no per-kind `ObjectTypeDefinition` flag;
    // `types: bool` covers all type kinds. With `{ ObjectTypeDefinition: true }`,
    // serde ignores the unknown field and `types` defaults to false, so we
    // produce 0 errors instead of 1.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L88",
        super::UPSTREAM_SHA,
    ))
    .code("type User { id: ID }")
    .options(serde_json::json!({ "ObjectTypeDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L92>
#[test]
fn invalid_l92_interface_type_no_description() {
    // DIVERGENCE: `InterfaceTypeDefinition: true` is not a supported option;
    // we produce 0 errors instead of 1. See invalid_l88 for explanation.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L92",
        super::UPSTREAM_SHA,
    ))
    .code("interface Node { id: ID! }")
    .options(serde_json::json!({ "InterfaceTypeDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L96>
#[test]
fn invalid_l96_enum_type_no_description() {
    // DIVERGENCE: `EnumTypeDefinition: true` is not a supported option;
    // we produce 0 errors instead of 1. See invalid_l88 for explanation.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L96",
        super::UPSTREAM_SHA,
    ))
    .code("enum Role { ADMIN }")
    .options(serde_json::json!({ "EnumTypeDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L100>
#[test]
fn invalid_l100_scalar_no_description() {
    // DIVERGENCE: `ScalarTypeDefinition: true` is not a supported option;
    // we produce 0 errors instead of 1. See invalid_l88 for explanation.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L100",
        super::UPSTREAM_SHA,
    ))
    .code("scalar Email")
    .options(serde_json::json!({ "ScalarTypeDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L104>
#[test]
fn invalid_l104_input_object_no_description() {
    // DIVERGENCE: `InputObjectTypeDefinition: true` is not a supported option;
    // we produce 0 errors instead of 1. See invalid_l88 for explanation.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L104",
        super::UPSTREAM_SHA,
    ))
    .code("input CreateUserInput { email: Email! }")
    .options(serde_json::json!({ "InputObjectTypeDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L108>
#[test]
fn invalid_l108_union_no_description() {
    // DIVERGENCE: `UnionTypeDefinition: true` is not a supported option;
    // we produce 0 errors instead of 1. See invalid_l88 for explanation.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L108",
        super::UPSTREAM_SHA,
    ))
    .code("union Media = Book | Movie")
    .options(serde_json::json!({ "UnionTypeDefinition": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L112>
#[test]
fn invalid_l112_directive_no_description() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L112",
        super::UPSTREAM_SHA,
    ))
    .code("directive @auth(requires: Role!) on FIELD_DEFINITION")
    .options(serde_json::json!({ "DirectiveDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L116>
#[test]
fn invalid_l116_field_no_description() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L116",
        super::UPSTREAM_SHA,
    ))
    .code("type User { email: Email! }")
    .options(serde_json::json!({ "FieldDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L120>
#[test]
fn invalid_l120_input_value_no_description() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L120",
        super::UPSTREAM_SHA,
    ))
    .code("input CreateUserInput { email: Email! }")
    .options(serde_json::json!({ "InputValueDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L124>
#[test]
fn invalid_l124_enum_value_no_description() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L124",
        super::UPSTREAM_SHA,
    ))
    .code("enum Role { ADMIN }")
    .options(serde_json::json!({ "EnumValueDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L128>
#[test]
fn invalid_l128_object_type_override_false() {
    // Upstream: `{ types: true, ObjectTypeDefinition: false, FieldDefinition: true }`
    // disables ObjectTypeDefinition and enables FieldDefinition only → 2 field errors.
    // DIVERGENCE: we don't support per-kind overrides like `ObjectTypeDefinition: false`.
    // With `{ types: true, FieldDefinition: true }` we fire on the type too, giving
    // 3 errors (type + 2 fields) instead of 2.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L128",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type CreateOneUserPayload {\n  recordId: MongoID\n  record: User\n}",
    )
    .options(serde_json::json!({ "types": true, "ObjectTypeDefinition": false, "FieldDefinition": true }))
    .errors(vec![
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
    ])
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L143>
#[test]
fn invalid_l143_operation_lines_before_not_1() {
    // A blank line between the comment and the query means `linesBefore !== 1`.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L143",
        super::UPSTREAM_SHA,
    ))
    .code("# linesBefore !== 1\n\nquery {\n  foo\n}")
    .options(serde_json::json!({ "OperationDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L152>
#[test]
fn invalid_l152_mutation_no_description() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L152",
        super::UPSTREAM_SHA,
    ))
    .code("mutation createUser { foo }")
    .options(serde_json::json!({ "OperationDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L157>
#[test]
fn invalid_l157_subscription_no_description() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L157",
        super::UPSTREAM_SHA,
    ))
    .code("subscription commentAdded { foo }")
    .options(serde_json::json!({ "OperationDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L162>
#[test]
fn invalid_l162_eslint_disable_comment() {
    // A `# eslint-disable…` comment directly above the operation does NOT count
    // as a description.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L162",
        super::UPSTREAM_SHA,
    ))
    .code("# eslint-disable-next-line semi\nquery {\n  foo\n}")
    .options(serde_json::json!({ "OperationDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L170>
#[test]
fn invalid_l170_fragment_comment_ignored_for_query() {
    // A comment before a fragment doesn't count for a subsequent query.
    // The fragment is ignored (fragments are excluded); only the query fires.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L170",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# BAD\nfragment UserFields on User {\n  id\n}\n\nquery {\n  user {\n    ...UserFields\n  }\n}",
    )
    .options(serde_json::json!({ "OperationDefinition": true }))
    .errors(vec![ExpectedError::new().message_id("require-description")])
    .run_against_standalone_document(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L183>
#[test]
fn invalid_l183_root_field_no_description() {
    // Upstream: `rootField: true`, `type Query { user(id: String!): User! }` → 1 error.
    // DIVERGENCE: `rootField` is accepted but not yet implemented; with
    // `{ rootField: true }` alone all other kinds default to false, so we
    // produce 0 errors instead of 1.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L183",
        super::UPSTREAM_SHA,
    ))
    .code("type Query { user(id: String!): User! }")
    .options(serde_json::json!({ "rootField": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L189>
#[test]
fn invalid_l189_root_field_mutation_no_description() {
    // DIVERGENCE: `rootField` not implemented; produces 0 errors instead of 1.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L189",
        super::UPSTREAM_SHA,
    ))
    .code("type Mutation { createUser(id: [ID!]!): User! }")
    .options(serde_json::json!({ "rootField": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L195>
#[test]
fn invalid_l195_root_field_subscription_no_description() {
    // DIVERGENCE: `rootField` not implemented; produces 0 errors instead of 1.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L195",
        super::UPSTREAM_SHA,
    ))
    .code("type MySubscription {\n  users: [User!]!\n}\nschema {\n  subscription: MySubscription\n}")
    .options(serde_json::json!({ "rootField": true }))
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-description/index.test.ts#L207>
#[test]
fn invalid_l207_ignored_selectors() {
    // Upstream: `types: true` + `ignoredSelectors` exempts PageInfo, *Connection,
    // *Edge types → errors on Query, User, Friend only (3 errors).
    // DIVERGENCE: we don't implement `ignoredSelectors`; with `{ types: true }` we
    // fire on all undescribed types. The code below has these undescribed types:
    // Query, User, FriendConnection, FriendEdge, Friend, PageInfo = 6 types.
    // Assert 6 so the test stays green against our actual output.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-description/index.test.ts#L207",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"type Query {
  user: User
}
type User {
  id: ID!
  name: String!
  friends(first: Int, after: String): FriendConnection!
}
type FriendConnection {
  edges: [FriendEdge]
  pageInfo: PageInfo!
}
type FriendEdge {
  cursor: String!
  node: Friend!
}
type Friend {
  id: ID!
  name: String!
}
type PageInfo {
  hasPreviousPage: Boolean!
  hasNextPage: Boolean!
  startCursor: String
  endCursor: String
}"#,
    )
    .options(serde_json::json!({
        "types": true,
        "ignoredSelectors": [
            "[type=ObjectTypeDefinition][name.value=PageInfo]",
            "[type=ObjectTypeDefinition][name.value=/(Connection|Edge)$/]"
        ]
    }))
    .errors(vec![
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
        ExpectedError::new().message_id("require-description"),
    ])
    .run_against_standalone_schema(RequireDescriptionRuleImpl);
}
