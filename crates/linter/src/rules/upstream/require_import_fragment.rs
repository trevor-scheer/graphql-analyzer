//! Verbatim port of `@graphql-eslint`'s `require-import-fragment` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts>
//!
//! Upstream's test is entirely file-based, reading `.gql` mocks from disk and
//! using cross-file project awareness to validate imports. We inline the mock
//! file contents as extra documents and run them through the standalone document
//! rule, which has access to `project_files` for cross-file lookup.

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
    // The rule resolves the path against the current file's URI, loads
    // `fragments/foo-fragment.gql` from project_files, and confirms that
    // `FooFields` is defined there — so no error is reported.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L38",
        super::UPSTREAM_SHA,
    ))
    .code(
        "# Imports could have extra whitespace and double/single quotes\n#  import  './fragments/foo-fragment.gql'\n\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .document(
        "fragments/foo-fragment.gql",
        "fragment FooFields on Foo { id }",
    )
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
    // `bar-fragment.gql` only defines `BarFields`. The rule resolves the path,
    // finds `bar-fragment.gql` in project_files, and confirms that `FooFields`
    // is not defined there — so it reports an error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L48",
        super::UPSTREAM_SHA,
    ))
    .code(
        "#import FooFields from \"./fragments/bar-fragment.gql\"\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .document(
        "fragments/bar-fragment.gql",
        "fragment BarFields on Bar { id }",
    )
    .errors(vec![ExpectedError::new().message(
        "Expected \"FooFields\" fragment to be imported.",
    )])
    .run_against_standalone_document(RequireImportFragmentRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-import-fragment/index.test.ts#L53>
#[test]
fn invalid_l53_default_import_wrong_file() {
    // `invalid-query-default.gql`: `#import './fragments/bar-fragment.gql'`
    // (default import pointing to the wrong file). The rule resolves the path,
    // finds `bar-fragment.gql` in project_files, checks whether `FooFields` is
    // defined there — it isn't (only `BarFields` is) — so it reports an error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-import-fragment/index.test.ts#L53",
        super::UPSTREAM_SHA,
    ))
    .code(
        "#import './fragments/bar-fragment.gql'\nquery {\n  foo {\n    ...FooFields\n  }\n}",
    )
    .document(
        "fragments/bar-fragment.gql",
        "fragment BarFields on Bar { id }",
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
