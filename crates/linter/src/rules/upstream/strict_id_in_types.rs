//! Verbatim port of `@graphql-eslint`'s `strict-id-in-types` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::strict_id_in_types::StrictIdInTypesRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L6>
#[test]
fn valid_l6_type_with_id_field() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L6",
        super::UPSTREAM_SHA,
    ))
    .code("type A { id: ID! }")
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L7>
#[test]
fn valid_l7_custom_id_name_and_type() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code("type A { _id: String! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["_id"],
        "acceptedIdTypes": ["String"],
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L16>
#[test]
fn valid_l16_multiple_accepted_id_names_and_types() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L16",
        super::UPSTREAM_SHA,
    ))
    .code("type A { _id: String! } type A1 { id: ID! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id", "_id"],
        "acceptedIdTypes": ["ID", "String"],
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L25>
#[test]
fn valid_l25_suffix_exception_result() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L25",
        super::UPSTREAM_SHA,
    ))
    .code("type A { id: ID! } type AResult { key: String! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "suffixes": ["Result"] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L37>
#[test]
fn valid_l37_empty_string_suffix_matches_all() {
    // An empty-string suffix matches every type name (every string ends with "").
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L37",
        super::UPSTREAM_SHA,
    ))
    .code("type A { id: ID! } type A1 { id: ID! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "suffixes": [""] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L49>
#[test]
fn valid_l49_explicit_default_options() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L49",
        super::UPSTREAM_SHA,
    ))
    .code("type A { id: ID! } type A1 { id: ID! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L58>
#[test]
fn valid_l58_multiple_suffix_exceptions() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L58",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type A { id: ID! } type AResult { key: String! } type APayload { bool: Boolean! } type APagination { num: Int! }",
    )
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "suffixes": ["Result", "Payload", "Pagination"] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L70>
#[test]
fn valid_l70_type_exception_by_name() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L70",
        super::UPSTREAM_SHA,
    ))
    .code("type A { id: ID! } type AError { message: String! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "types": ["AError"] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L82>
#[test]
fn valid_l82_multiple_type_exceptions() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L82",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type A { id: ID! } type AGeneralError { message: String! } type AForbiddenError { message: String! }",
    )
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "types": ["AGeneralError", "AForbiddenError"] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L95>
#[test]
fn valid_l95_empty_string_type_exception() {
    // An empty-string type name exception never matches a real type (type names
    // must be non-empty in valid GraphQL), so this effectively disables the
    // exception while still using the option.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L95",
        super::UPSTREAM_SHA,
    ))
    .code("type A { id: ID! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "types": [""] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L107>
#[test]
fn valid_l107_combined_type_and_suffix_exceptions() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L107",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type A { id: ID! } type AError { message: String! } type AResult { payload: A! }",
    )
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "types": ["AError"], "suffixes": ["Result"] },
    }))
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L120>
#[test]
fn valid_l120_ignores_root_types() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L120",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User {\n  id: ID!\n}\ntype Query {\n  user: User\n}\ntype Mutation {\n  createUser: User\n}\ntype Subscription {\n  userAdded: User\n}",
    )
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L137>
#[test]
fn valid_l137_ignores_renamed_root_types() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L137",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type User {\n  id: ID!\n}\ntype MyQuery {\n  user: User\n}\ntype MyMutation {\n  createUser: User\n}\ntype MySubscription {\n  userAdded: User\n}\nschema {\n  query: MyQuery\n  mutation: MyMutation\n  subscription: MySubscription\n}",
    )
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L161>
#[test]
fn invalid_l161_type_without_id_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L161",
        super::UPSTREAM_SHA,
    ))
    .code("type B { name: String! }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L165>
#[test]
fn invalid_l165_two_id_fields_both_accepted() {
    // Having two valid identifier fields (id and _id) is still a violation:
    // exactly one non-nullable identifier is required.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L165",
        super::UPSTREAM_SHA,
    ))
    .code("type B { id: ID! _id: String! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id", "_id"],
        "acceptedIdTypes": ["ID", "String"],
    }))
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L175>
#[test]
fn invalid_l175_list_wrapped_id_fields_not_accepted() {
    // Only `NonNullType<NamedType>` counts as a valid identifier. List wrappers
    // are not accepted regardless of nullability. B has `id: String!` which IS
    // valid; B1–B4 all use list wrapping so they each produce 1 error (4 total).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L175",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type B { id: String! } type B1 { id: [String] } type B2 { id: [String!] } type B3 { id: [String]! } type B4 { id: [String!]! }",
    )
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["String"],
    }))
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L185>
#[test]
fn invalid_l185_suffix_match_is_case_sensitive() {
    // Exception suffixes are case-sensitive: `Bresult` does not end with
    // `Result` (capital R), so it is invalid. `BPagination` has no matching
    // suffix either → 2 errors. `B` (valid id) and `BPayload` (suffix match)
    // are skipped.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L185",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type B { id: ID! } type Bresult { key: String! } type BPayload { bool: Boolean! } type BPagination { num: Int! }",
    )
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "suffixes": ["Result", "Payload"] },
    }))
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L198>
#[test]
fn invalid_l198_type_exception_does_not_match_different_name() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/strict-id-in-types/index.test.ts#L198",
        super::UPSTREAM_SHA,
    ))
    .code("type B { id: ID! } type BError { message: String! }")
    .options(serde_json::json!({
        "acceptedIdNames": ["id"],
        "acceptedIdTypes": ["ID"],
        "exceptions": { "types": ["GeneralError"] },
    }))
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(StrictIdInTypesRuleImpl);
}
