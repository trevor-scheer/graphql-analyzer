//! Verbatim port of `@graphql-eslint`'s `no-unused-variables` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/__tests__/no-unused-variables.spec.ts>
//!
//! The upstream test lives in `__tests__/` because `no-unused-variables` wraps
//! graphql-js's `NoUnusedVariablesRule` via the `graphql-js-validation`
//! adapter. There is exactly one valid case and no invalid cases.
//!
//! The valid case uses two mock documents:
//!   - `no-unused-variables.gql` (primary): declares `$limit` and `$offset`
//!     in the operation; they are NOT referenced directly in the operation
//!     body — only via a fragment spread.
//!   - `user-fields-with-variables.gql` (extra): the `UserFields` fragment
//!     uses `$limit`/`$offset` inside its field arguments.
//!
//! graphql-js resolves variable usage through fragment spreads (the full
//! merged document is validated as a unit). Our rule now also walks cross-file
//! fragment bodies transitively, so this case produces zero errors.

use super::harness::Case;
use crate::rules::no_unused_variables::NoUnusedVariablesRuleImpl;

/// Content of `packages/plugin/__tests__/mocks/no-unused-variables.gql`
const NO_UNUSED_VARIABLES: &str = "\
query ($limit: Int!, $offset: Int!) {\n\
  user {\n\
    id\n\
    ...UserFields\n\
  }\n\
}";

/// Content of `packages/plugin/__tests__/mocks/user-fields-with-variables.gql`
const USER_FIELDS_WITH_VARIABLES: &str = "\
fragment UserFields on User {\n\
  firstName\n\
  posts(limit: $limit, offset: $offset) {\n\
    id\n\
  }\n\
}";

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/__tests__/no-unused-variables.spec.ts#L7>
#[test]
fn valid_l7_variables_used_in_cross_file_fragment() {
    // $limit and $offset are declared in the operation but only referenced
    // inside the UserFields fragment body, which lives in a separate file.
    // The rule walks fragment spreads transitively, so both variables are
    // found used and no diagnostic is emitted.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/__tests__/no-unused-variables.spec.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code(NO_UNUSED_VARIABLES)
    .document("user-fields-with-variables.gql", USER_FIELDS_WITH_VARIABLES)
    .run_against_standalone_document(NoUnusedVariablesRuleImpl);
}
