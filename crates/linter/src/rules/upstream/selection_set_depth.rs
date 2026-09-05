//! Verbatim port of `@graphql-eslint`'s `selection-set-depth` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::selection_set_depth::SelectionSetDepthRuleImpl;

/// Fragment used as a sibling document in `WITH_SIBLINGS` cases.
const ALBUM_FIELDS_FRAGMENT: &str = "fragment AlbumFields on Album { id }";

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L14>
#[test]
fn valid_l14_anon_query_within_max_depth() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L14",
        super::UPSTREAM_SHA,
    ))
    .code(
        "{\n  viewer {\n    albums {\n      title\n    }\n  }\n}",
    )
    .options(serde_json::json!({ "maxDepth": 2 }))
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L26>
#[test]
fn valid_l26_fragment_spread_inlined_within_depth() {
    // Upstream: maxDepth=2, AlbumFields = { id } (depth 1 inside fragment).
    // Inlined: viewer(0) → albums(1) → id(2 ≤ 2) → valid.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L26",
        super::UPSTREAM_SHA,
    ))
    .code("query deep2 {\n  viewer {\n    albums {\n      ...AlbumFields\n    }\n  }\n}")
    .document("album-fields.graphql", ALBUM_FIELDS_FRAGMENT)
    .options(serde_json::json!({ "maxDepth": 2 }))
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L39>
#[test]
fn valid_l39_ignore_skips_field_and_subtree() {
    // `albums` is in the ignore list: the field itself and its subtree are
    // excluded from depth counting. viewer is at depth 0 (≤ 1) → valid.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L39",
        super::UPSTREAM_SHA,
    ))
    .code("query deep2 {\n  viewer {\n    albums {\n      ...AlbumFields\n    }\n  }\n}")
    .document("album-fields.graphql", ALBUM_FIELDS_FRAGMENT)
    .options(serde_json::json!({ "maxDepth": 1, "ignore": ["albums"] }))
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L54>
#[test]
fn invalid_l54_named_query_exceeds_max_depth() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L54",
        super::UPSTREAM_SHA,
    ))
    .code("query deep2 {\n  viewer {\n    albums {\n      title\n    }\n  }\n}")
    .options(serde_json::json!({ "maxDepth": 1 }))
    .errors(vec![
        ExpectedError::new().message("'deep2' exceeds maximum operation depth of 1"),
    ])
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L67>
#[test]
fn invalid_l67_fragment_spread_inlined_exceeds_depth() {
    // AlbumFields = { id } is inlined at the spread site.
    // viewer(0) → albums(1) → id(2 > 1) → error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L67",
        super::UPSTREAM_SHA,
    ))
    .code("query deep2 {\n  viewer {\n    albums {\n      ...AlbumFields\n    }\n  }\n}")
    .document("album-fields.graphql", ALBUM_FIELDS_FRAGMENT)
    .options(serde_json::json!({ "maxDepth": 1 }))
    .errors(vec![
        ExpectedError::new().message("'deep2' exceeds maximum operation depth of 1"),
    ])
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L81>
#[test]
fn invalid_l81_inline_fragment_does_not_add_depth() {
    // Inline fragments are transparent to depth (they forward at the same level).
    // viewer(0) → albums(1) → inline fragment (same 1) → id(2 > 1) → error.
    // Upstream names this "suggestions should work with inline fragments".
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L81",
        super::UPSTREAM_SHA,
    ))
    .code(
        "{\n  viewer {\n    albums {\n      ... on Album {\n        id\n      }\n    }\n  }\n}",
    )
    .document("album-fields.graphql", ALBUM_FIELDS_FRAGMENT)
    .options(serde_json::json!({ "maxDepth": 1 }))
    .errors(vec![
        ExpectedError::new().message("'' exceeds maximum operation depth of 1"),
    ])
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/selection-set-depth/index.test.ts#L98>
#[test]
fn invalid_l98_deep_fragment_in_different_file_exceeds_depth() {
    // AlbumFields = { id  modifier { date } } is inlined at the spread site.
    // viewer(0) → albums(1) → modifier(2) → date(3 > 2) → error.
    const DEEP_ALBUM_FIELDS: &str = "\
fragment AlbumFields on Album {
  id
  modifier {
    date
  }
}";
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/selection-set-depth/index.test.ts#L98",
        super::UPSTREAM_SHA,
    ))
    .code("{\n  viewer {\n    albums {\n      ...AlbumFields\n    }\n  }\n}")
    .document("album-fields.graphql", DEEP_ALBUM_FIELDS)
    .options(serde_json::json!({ "maxDepth": 2 }))
    .errors(vec![
        ExpectedError::new().message("'' exceeds maximum operation depth of 2"),
    ])
    .run_against_standalone_document(SelectionSetDepthRuleImpl);
}
