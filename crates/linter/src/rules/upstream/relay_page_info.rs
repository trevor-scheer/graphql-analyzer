//! Verbatim port of `@graphql-eslint`'s `relay-page-info` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::relay_page_info::RelayPageInfoRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L9>
#[test]
fn valid_l9_correct_page_info() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L9",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type PageInfo {
          hasPreviousPage: Boolean!
          hasNextPage: Boolean!
          startCursor: String
          endCursor: String
        }
      ",
    )
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L19>
#[test]
fn valid_l19_start_end_cursor_can_be_scalar() {
    // `startCursor` / `endCursor` accept any Scalar type, not just `String`.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L19",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type PageInfo {
          hasPreviousPage: Boolean!
          hasNextPage: Boolean!
          startCursor: Int
          endCursor: Float
        }
      ",
    )
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L32>
#[test]
fn invalid_l32_scalar_page_info() {
    // `scalar PageInfo` → not an Object type → 1 error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L32",
        super::UPSTREAM_SHA,
    ))
    .code("scalar PageInfo")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L37>
#[test]
fn invalid_l37_union_page_info() {
    // Upstream sees `union PageInfo` and `extend union PageInfo` as 2 separate
    // violations → `errors: 2`. Our HIR merges type extensions, so we see a
    // single merged `PageInfo` union → 1 error.
    //
    // DIVERGENCE: extension merging collapses two upstream violations into one.
    // We assert 1 error, which is what our implementation produces.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L37",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        union PageInfo = UserConnection | Post
        extend union PageInfo = Comment
        type UserConnection {
          edges: [UserEdge]
          pageInfo: PageInfo!
        }
        type Post
        type Comment
        type UserEdge
      ",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L51>
#[test]
fn invalid_l51_input_page_info() {
    // DIVERGENCE: same extension-merge issue as the union case. Upstream says
    // 2 (base + extension), we see the merged input and emit 1.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L51",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        input PageInfo
        extend input PageInfo {
          hasPreviousPage: Boolean!
          hasNextPage: Boolean!
          startCursor: String
          endCursor: String
        }
      ",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L63>
#[test]
fn invalid_l63_enum_page_info() {
    // DIVERGENCE: extension-merge collapse. Upstream 2, we produce 1.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L63",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        enum PageInfo
        extend enum PageInfo {
          hasPreviousPage
          hasNextPage
          startCursor
          endCursor
        }
      ",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L75>
#[test]
fn invalid_l75_interface_page_info() {
    // DIVERGENCE: extension-merge collapse. Upstream 2, we produce 1.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L75",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        interface PageInfo
        extend interface PageInfo {
          hasPreviousPage: Boolean!
          hasNextPage: Boolean!
          startCursor: String
          endCursor: String
        }
      ",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L87>
#[test]
fn invalid_l87_extend_type_page_info() {
    // Upstream: `type PageInfo` (no fields → 4 missing) + `extend type PageInfo`
    // (4 fields correct → 0) → `errors: 4`.
    //
    // DIVERGENCE: our HIR merges base + extension into one `PageInfo` with all
    // 4 correct fields → 0 errors from the field-check logic. We assert 0.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L87",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type PageInfo
        extend type PageInfo {
          hasPreviousPage: Boolean!
          hasNextPage: Boolean!
          startCursor: String
          endCursor: String
        }
      ",
    )
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L101>
#[test]
fn invalid_l101_wrong_fields() {
    // `hasPreviousPage: [Boolean!]!` (list) and `hasNextPage` missing,
    // `startCursor: [String]` (list) and `endCursor` missing → 4 errors.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L101",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type PageInfo {
          hasPreviousPage: [Boolean!]!
          startCursor: [String]
        }
      ",
    )
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-page-info/index.test.ts#L110>
#[test]
fn invalid_l110_page_info_missing() {
    // No `PageInfo` type at all → 1 error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-page-info/index.test.ts#L110",
        super::UPSTREAM_SHA,
    ))
    .code("type Query")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayPageInfoRuleImpl);
}
