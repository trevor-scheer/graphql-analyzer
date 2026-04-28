//! Verbatim port of `@graphql-eslint`'s `require-import-fragment` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts>
//!
//! Upstream's test is entirely file-based, reading `.gql` mocks from disk and
//! using cross-file project awareness to validate imports. We inline the mock
//! file contents and run them through the standalone document rule.
//!
//! # Divergences
//!
//! Our rule is `StandaloneDocumentLintRule`, not a project-aware rule.
//! Two categories of upstream behavior we don't replicate:
//!
//! 1. **Default imports** (`# import 'path/to/file.gql'` without a fragment name):
//!    upstream resolves the path and imports all fragments defined there.
//!    Our rule doesn't support this syntax; `FooFields` stays unimported.
//!    Cases `valid_l...default_import` and `invalid_l...default_import` diverge
//!    for this reason.
//!
//! 2. **Cross-file import validation**: upstream checks that the imported
//!    fragment is actually defined in the referenced file. `invalid-query.gql`
//!    has `#import FooFields from "bar-fragment.gql"` where `bar-fragment.gql`
//!    only defines `BarFields` — upstream fires because the fragment is missing
//!    from the imported file. Our rule only checks that a `# import FooFields`
//!    comment exists (any path), so we consider it imported and produce no error.

use super::harness::{Case, ExpectedError};
use crate::rules::require_import_fragment::RequireImportFragmentRuleImpl;

// Contents of the upstream mock files (reproduced verbatim):
//
//   valid-query.gql:
//     # Imports could have extra whitespace and double/single quotes
//     #  import  FooFields  from  "./fragments/foo-fragment.gql"
//     query { foo { ...FooFields } }
//
//   valid-query-default.gql:
//     # Imports could have extra whitespace and double/single quotes
//     #  import  './fragments/foo-fragment.gql'
//     query { foo { ...FooFields } }
//
//   same-file.gql:
//     { foo { ...FooFields } }
//     fragment FooFields on Foo { id }
//
//   invalid-query.gql:
//     #import FooFields from "./fragments/bar-fragment.gql"
//     query { foo { ...FooFields } }
//
//   invalid-query-default.gql:
//     #import './fragments/bar-fragment.gql'
//     query { foo { ...FooFields } }
//
//   missing-import.gql:
//     { foo { ...FooFields } }

// ---------------------------------------------------------------------------
// Valid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L33>
#[test]
fn valid_l33_named_import_with_extra_whitespace() {
    // `valid-query.gql`: extra whitespace around `import` and fragment name.
    // Our parser trims correctly so `FooFields` is recognised as imported.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# Imports could have extra whitespace and double/single quotes\n#  import  FooFields  from  \"./fragments/foo-fragment.gql\"\n\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L38>
#[test]
fn valid_l38_default_import() {
    // `valid-query-default.gql`: `# import 'path'` without a named fragment.
    // DIVERGENCE: upstream resolves the path and treats all fragments in that
    // file as imported. Our rule requires the fragment name in the import
    // comment; `FooFields` is not found so we produce 1 error instead of 0.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L38",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# Imports could have extra whitespace and double/single quotes\n#  import  './fragments/foo-fragment.gql'\n\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .errors(vec![ExpectedError::new().message(
        "Expected \"FooFields\" fragment to be imported.",
    )])
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L43>
#[test]
fn valid_l43_same_file_fragment() {
    // `same-file.gql`: fragment defined in the same document, no import needed.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L43",
        super::UPSTREAM_SHA,
    ))
    .code("{\n  foo {\n    ...FooFields\n  }\n}\n\nfragment FooFields on Foo {\n  id\n}")
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}

// ---------------------------------------------------------------------------
// Invalid cases
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L48>
#[test]
fn invalid_l48_named_import_wrong_file() {
    // `invalid-query.gql`: imports `FooFields` from `bar-fragment.gql`, but
    // `bar-fragment.gql` only defines `BarFields`.
    // DIVERGENCE: upstream fires because the fragment isn't in the imported file.
    // Our rule only checks for the presence of `# import FooFields …` (any path);
    // the comment `#import FooFields from "bar-fragment.gql"` satisfies our parser
    // (no space after `#` is fine), so we produce 0 errors instead of 1.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L48",
        super::UPSTREAM_SHA,
    ))
    .code(
        "#import FooFields from \"./fragments/bar-fragment.gql\"\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L53>
#[test]
fn invalid_l53_default_import_wrong_file() {
    // `invalid-query-default.gql`: `#import './fragments/bar-fragment.gql'`
    // (default import pointing to wrong file).
    // Our rule doesn't recognise default imports so `FooFields` is not found
    // and we fire — the error count coincidentally matches upstream (1), but
    // the root cause differs: upstream fires because the import points to the
    // wrong file; we fire because the fragment name isn't in any import comment.
    // DIVERGENCE: upstream error message differs from ours but count matches.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L53",
        super::UPSTREAM_SHA,
    ))
    .code(
        "#import './fragments/bar-fragment.gql'\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .errors(vec![ExpectedError::new().message(
        "Expected \"FooFields\" fragment to be imported.",
    )])
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L58>
#[test]
fn invalid_l58_missing_import() {
    // `missing-import.gql`: no import comment at all.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L58",
        super::UPSTREAM_SHA,
    ))
    .code("{\n  foo {\n    ...FooFields\n  }\n}")
    .errors(vec![ExpectedError::new().message(
        "Expected \"FooFields\" fragment to be imported.",
    )])
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}
