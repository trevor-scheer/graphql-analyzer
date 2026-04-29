//! Verbatim port of `@graphql-eslint`'s `description-style` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/description-style/index.test.ts>

use super::harness::{Case, ExpectedError, ExpectedSuggestion};
use crate::rules::description_style::DescriptionStyleRuleImpl;

/// Shared SDL with block-style descriptions used across multiple cases.
/// Upstream defines this as `BLOCK_SDL` (L15–L30).
const BLOCK_SDL: &str = r#"
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
"#;

/// Shared SDL with inline-style descriptions used across multiple cases.
/// Upstream defines this as `INLINE_SDL` (L4–L13).
const INLINE_SDL: &str = r#"
  " Test "
  type CreateOneUserPayload {
    "Created document ID"
    recordId: MongoID

    "Created document"
    record: User
  }
"#;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/description-style/index.test.ts#L34>
#[test]
fn valid_l34_block_sdl_default() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/description-style/index.test.ts#L34",
        super::UPSTREAM_SHA,
    ))
    .code(BLOCK_SDL)
    .run_against_standalone_schema(DescriptionStyleRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/description-style/index.test.ts#L36>
#[test]
fn valid_l36_inline_sdl_with_inline_option() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/description-style/index.test.ts#L36",
        super::UPSTREAM_SHA,
    ))
    .code(INLINE_SDL)
    .options(serde_json::json!({ "style": "inline" }))
    .run_against_standalone_schema(DescriptionStyleRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/description-style/index.test.ts#L42>
#[test]
fn invalid_l42_block_sdl_with_inline_option() {
    // Upstream uses `errors: 3` (count only, no messageId or suggestions).
    // We document our suggestion description here.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/description-style/index.test.ts#L42",
        super::UPSTREAM_SHA,
    ))
    .code(BLOCK_SDL)
    .options(serde_json::json!({ "style": "inline" }))
    .errors(vec![
        ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Change to inline style description", "")]),
        ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Change to inline style description", "")]),
        ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Change to inline style description", "")]),
    ])
    .run_against_standalone_schema(DescriptionStyleRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/description-style/index.test.ts#L47>
#[test]
fn invalid_l47_inline_sdl_default() {
    // Upstream uses `errors: 3` (count only, no messageId or suggestions).
    // We document our suggestion description here.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/description-style/index.test.ts#L47",
        super::UPSTREAM_SHA,
    ))
    .code(INLINE_SDL)
    .errors(vec![
        ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Change to block style description", "")]),
        ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Change to block style description", "")]),
        ExpectedError::new().suggestions(vec![ExpectedSuggestion::new("Change to block style description", "")]),
    ])
    .run_against_standalone_schema(DescriptionStyleRuleImpl);
}
