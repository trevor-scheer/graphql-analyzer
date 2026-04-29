//! Verbatim port of `@graphql-eslint`'s `require-selections` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts>
//!
//! # Option mapping
//!
//! Upstream's rule option is `{ fieldName: string | string[], requireAllFields?: boolean }`.
//! Our rule mirrors this: `fieldName` is the primary key with OR semantics (any one of the
//! listed names satisfies the check); `requireAllFields: true` switches to AND semantics
//! with one diagnostic per missing field. The `fields` key is accepted as a deprecated
//! alias for `fieldName`.
//!
//! Upstream `{ fieldName: 'x' }` maps to our `{ "fieldName": ["x"] }`.
//! Upstream `{ fieldName: ['a', 'b'] }` uses OR semantics — any one of the listed
//! names satisfies the requirement. Upstream `{ requireAllFields: true, fieldName: [...] }`
//! requires every listed field and emits one error per missing field.

use super::harness::{Case, ExpectedError};
use crate::rules::require_selections::RequireSelectionsRuleImpl;

const TEST_SCHEMA: &str = "\
type Query {
  hasId: HasId!
  noId: NoId!
  vehicles: [Vehicle!]!
  flying: [Flying!]!
  noIdOrNoId2: NoIdOrNoId2!
}

type NoId {
  name: String!
}

type NoId2 {
  name: String!
}

union NoIdOrNoId2 = NoId | NoId2

interface Vehicle {
  id: ID!
}

type Car implements Vehicle {
  id: ID!
  mileage: Int
}

interface Flying {
  hasWings: Boolean!
}

type Bird implements Flying {
  id: ID!
  hasWings: Boolean!
}

type HasId {
  id: ID!
  _id: ID!
  name: String!
}
";

const USER_POST_SCHEMA: &str = "\
type User {
  id: ID
  name: String
  posts: [Post]
}

type Post {
  id: ID
  title: String
  author: [User!]!
}

type Query {
  user: User
  userOrPost: UserOrPost
}

union UserOrPost = User | Post
";

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L68>
#[test]
fn valid_l68_ignore_operation_definition_check() {
    // Checking selections on the OperationDefinition itself is redundant; the rule
    // should not fire on `{ foo }` even when `Query` has no `id` field.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L68",
        super::UPSTREAM_SHA,
    ))
    .schema("type Query { id: ID }")
    .code("{ foo }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L77>
#[test]
fn valid_l77_no_id_type_no_error() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L77",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ noId { name } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L78>
#[test]
fn valid_l78_has_id_selected() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L78",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { id name } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L79>
#[test]
fn valid_l79_id_found_in_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L79",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { ...HasIdFields } }")
    .document("fragments.graphql", "fragment HasIdFields on HasId { id }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L88>
#[test]
fn valid_l88_vehicles_with_id_and_inline_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L88",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ vehicles { id ...on Car { id mileage } } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L89>
#[test]
fn valid_l89_vehicles_id_via_inline_fragment_only() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L89",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ vehicles { ...on Car { id mileage } } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L90>
#[test]
fn valid_l90_flying_id_via_inline_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L90",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ flying { ...on Bird { id } } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L91>
#[test]
fn valid_l91_custom_field_name_option() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L91",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { name } }")
    .options(serde_json::json!({ "fields": ["name"] }))
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L96>
#[test]
fn valid_l96_vehicles_id_via_inline_no_direct_id() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L96",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ vehicles { id ...on Car { mileage } } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L100>
#[test]
fn valid_l100_multiple_id_field_names_any_one_sufficient() {
    // `{ fieldName: ['id', '_id'] }` with OR semantics — selecting `_id`
    // satisfies the requirement because any one candidate is sufficient.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L100",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { _id } }")
    .options(serde_json::json!({ "fieldName": ["id", "_id"] }))
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L105>
#[test]
fn valid_l105_nested_fragments_with_id() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L105",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserLightFields on User { id }\nfragment UserFullFields on User { ...UserLightFields\n  name\n}",
    )
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L126>
#[test]
fn valid_l126_nested_fragments_n_levels() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L126",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserLightFields on User { id }\nfragment UserMediumFields on User { ...UserLightFields\n  name\n}\nfragment UserFullFields on User { ...UserMediumFields\n  name\n}",
    )
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L151>
#[test]
fn valid_l151_nested_inline_fragments_n_levels() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L151",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserLightFields on User { ... on User { id } }\nfragment UserMediumFields on User { ...UserLightFields }\nfragment UserFullFields on User { ...UserMediumFields }",
    )
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L173>
#[test]
fn valid_l173_fragment_spread_inside_inline_fragments_n_levels() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L173",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserFields on User { id }\nfragment UserLightFields on User { ... on User { ...UserFields\n    name\n  } }\nfragment UserMediumFields on User { name\n  ...UserLightFields }\nfragment UserFullFields on User { name\n  ...UserMediumFields }",
    )
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L203>
#[test]
fn valid_l203_id_selected_after_fragment_spread() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L203",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserFields on User { name }\nfragment UserFullFields on User { ... on User { ...UserFields\n    id\n  } }",
    )
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L224>
#[test]
fn valid_l224_id_selected_after_inline_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L224",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserFields on User { name }\nfragment UserFullFields on User { ... on User { ...UserFields }\n  id\n}",
    )
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L245>
#[test]
fn valid_l245_id_via_alias() {
    // Upstream: `id: name` (alias `id` over field `name`) counts as selecting `id`.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L245",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { id: name } }")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L249>
#[test]
fn valid_l249_union_no_id_field_available() {
    // When none of the union members have the required field, no error should fire.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L249",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{\n  noIdOrNoId2 {\n    ... on NoId {\n      name\n    }\n    ... on NoId2 {\n      name\n    }\n  }\n}")
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L261>
#[test]
fn invalid_l261_missing_id_on_has_id() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L261",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { name } }")
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L266>
#[test]
fn invalid_l266_custom_field_name_missing() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L266",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { id } }")
    .options(serde_json::json!({ "fields": ["name"] }))
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L272>
#[test]
fn invalid_l272_multiple_id_field_names_none_selected() {
    // `{ fieldName: ['id', '_id'] }` on `{ hasId { name } }` → 1 error because
    // neither `id` nor `_id` was selected (OR check: none of the candidates present).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L272",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { name } }")
    .options(serde_json::json!({ "fieldName": ["id", "_id"] }))
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L279>
#[test]
fn invalid_l279_nested_fragments_no_id_anywhere() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L279",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("query User {\n  user {\n    ...UserFullFields\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UserLightFields on User { name }\nfragment UserMediumFields on User { ...UserLightFields\n  name\n}\nfragment UserFullFields on User { ...UserMediumFields\n  name\n}",
    )
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L303>
#[test]
fn invalid_l303_missing_posts_id_in_fragment() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L303",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("{ user { id ...UserFields } }")
    .document("fragments.graphql", "fragment UserFields on User { posts { title } }")
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L311>
#[test]
fn invalid_l311_multiple_missing_id_fields_nested() {
    // Four nested selection sets all missing `id`.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L311",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("{ user { ...UserFullFields } }")
    .document(
        "fragments.graphql",
        "fragment UserFullFields on User { posts { author { ...UserFields\n    authorPosts: posts { title } } } }\nfragment UserFields on User { name }",
    )
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
        ExpectedError::new().message_id("require-selections"),
        ExpectedError::new().message_id("require-selections"),
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L335>
#[test]
fn invalid_l335_union_missing_id() {
    let document_with_union = "{\n  userOrPost {\n    ... on User {\n      title\n    }\n  }\n}";
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L335",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code(document_with_union)
    .document("doc.graphql", document_with_union)
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L345>
#[test]
fn invalid_l345_union_with_fragment_spread_missing_id() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L345",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("{\n  userOrPost {\n    ... on User {\n      ...UserFields\n    }\n  }\n}")
    .document("fragments.graphql", "fragment UserFields on User { name }")
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L358>
#[test]
fn invalid_l358_union_non_inline_fragment_missing_id() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L358",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("{\n  userOrPost {\n    ...UnionFragment\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UnionFragment on UserOrPost { ... on User { name } }",
    )
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L371>
#[test]
fn invalid_l371_union_non_inline_fragment_nested_missing_id() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L371",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("{\n  userOrPost {\n    ...UnionFragment\n  }\n}")
    .document(
        "fragments.graphql",
        "fragment UnionFragment on UserOrPost { ...UserFields }\nfragment UserFields on User { name }",
    )
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L388>
#[test]
fn invalid_l388_fragment_definition_missing_id() {
    // Rule checks FragmentDefinitions too, not just operations.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L388",
        super::UPSTREAM_SHA,
    ))
    .schema(USER_POST_SCHEMA)
    .code("fragment UserFields on User {\n  name\n  posts {\n    title\n  }\n}")
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L399>
#[test]
fn invalid_l399_require_all_fields_option() {
    // `{ requireAllFields: true, fieldName: ['name', '_id'] }` on
    // `{ hasId { id } }` → 2 errors (both `name` and `_id` missing, one per field).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L399",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { id } }")
    .options(serde_json::json!({ "fieldName": ["name", "_id"], "requireAllFields": true }))
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-selections/index.test.ts#L407>
#[test]
fn invalid_l407_require_all_fields_partial_selection() {
    // `{ requireAllFields: true, fieldName: ['name', '_id'] }` on `{ hasId { _id } }` →
    // 1 error (`name` missing; `_id` is present so only `name` fires).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-selections/index.test.ts#L407",
        super::UPSTREAM_SHA,
    ))
    .schema(TEST_SCHEMA)
    .code("{ hasId { _id } }")
    .options(serde_json::json!({ "fieldName": ["name", "_id"], "requireAllFields": true }))
    .errors(vec![
        ExpectedError::new().message_id("require-selections"),
    ])
    .run_against_document_schema(RequireSelectionsRuleImpl);
}
