//! Verbatim port of `@graphql-eslint`'s `no-hashtag-description` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts>

use super::harness::Case;
use super::harness::ExpectedError;
use crate::rules::no_hashtag_description::NoHashtagDescriptionRuleImpl;

// ---------------------------------------------------------------------------
// valid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L8>
/// String description (double-quoted) is fine.
#[test]
fn valid_l8_string_description() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L8",
        super::UPSTREAM_SHA,
    ))
    .code(
        "\" Good \"\ntype Query {\n  foo: String\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L15>
/// Comment separated from definition by blank line is OK.
#[test]
fn valid_l15_comment_separated_by_blank_line() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L15",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# Good\n\ntype Query {\n  foo: String\n}\n# Good\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L23>
/// `#import` directive-style comment is OK (not a description).
#[test]
fn valid_l23_import_comment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L23",
        super::UPSTREAM_SHA,
    ))
    .code(
        "#import t\n\ntype Query {\n  foo: String\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L30>
/// Multiline comment block separated from definition by blank line is OK.
#[test]
fn valid_l30_multiline_separated_by_blank_line() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L30",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# multiline\n# multiline\n# multiline\n\ntype Query {\n  foo: String\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L38>
/// Inline (trailing) comments on definition lines are fine.
#[test]
fn valid_l38_inline_trailing_comments() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L38",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query { # Good\n  foo: String # Good\n} # Good\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L44>
/// `# eslint-disable-next-line` is an ESLint directive, not a hashtag
/// description. The suppression fires on the directive's own line, so the
/// rule does not produce a diagnostic.
#[test]
fn valid_l44_eslint_disable_next_line() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L44",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# eslint-disable-next-line\ntype Query {\n  foo: String\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L51>
/// Comment inside a type body followed by blank line before field is OK.
#[test]
fn valid_l51_comment_inside_body_followed_by_blank() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L51",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  # Good\n\n  foo: ID\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L59>
/// Comment between fields followed by blank line before next field is OK.
#[test]
fn valid_l59_comment_between_fields_with_blank() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L59",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  foo: ID\n  # Good\n\n  bar: ID\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L68>
/// Argument preceded by a comment with blank line is OK.
#[test]
fn valid_l68_comment_before_argument_with_blank() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L68",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  user(\n    # Good\n\n    id: Int\n  ): User\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L78>
/// Comment before an anonymous operation is OK (operations can't have descriptions).
#[test]
fn valid_l78_comment_before_anonymous_query() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L78",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# ok\nquery {\n  test\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L84>
#[test]
fn valid_l84_comment_before_mutation() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L84",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# ok\nmutation {\n  test\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L90>
#[test]
fn valid_l90_comment_before_subscription() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L90",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# ok\nsubscription {\n  test\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L96>
#[test]
fn valid_l96_comment_before_fragment() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L96",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# ok\nfragment UserFields on User {\n  id\n}\n",
    )
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

// ---------------------------------------------------------------------------
// invalid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L103>
#[test]
fn invalid_l103_hashtag_before_type() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L103",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# Bad\ntype Query {\n  foo: String\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message_id("HASHTAG_COMMENT"),
    ])
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L111>
/// Multiline hashtag block immediately before type fires one diagnostic.
#[test]
fn invalid_l111_multiline_hashtag_before_type() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L111",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# multiline\n# multiline\ntype Query {\n  foo: String\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message_id("HASHTAG_COMMENT"),
    ])
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L120>
/// Hashtag immediately before a field inside a type body.
#[test]
fn invalid_l120_hashtag_before_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L120",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  # Bad\n  foo: String\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message_id("HASHTAG_COMMENT"),
    ])
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L128>
/// Hashtag between fields: the one immediately before `foo` fires, not the
/// trailing one after `foo`.
#[test]
fn invalid_l128_hashtag_between_fields() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L128",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  bar: ID\n  # Bad\n  foo: ID\n  # Good\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message_id("HASHTAG_COMMENT"),
    ])
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L137>
/// Hashtag immediately before an argument.
#[test]
fn invalid_l137_hashtag_before_argument() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-hashtag-description/index.test.ts#L137",
        super::UPSTREAM_SHA,
    ))
    .code(
        "type Query {\n  user(\n    # Bad\n    id: Int!\n  ): User\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message_id("HASHTAG_COMMENT"),
    ])
    .run_against_standalone_schema(NoHashtagDescriptionRuleImpl);
}
