//! Verbatim port of `@graphql-eslint`'s `no-unused-fragments` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/__tests__/no-unused-fragments.spec.ts>
//!
//! The upstream test lives in `__tests__/` (not inside a rule directory) because
//! `no-unused-fragments` wraps graphql-js's `NoUnusedFragmentsRule` via the
//! `graphql-js-validation` adapter. It has exactly one valid case and no
//! invalid cases.
//!
//! The valid case uses three mock documents:
//!   - `user-fields.graphql`: defines `fragment UserFields on User { id firstName }`
//!   - `post-fields.graphql`: defines `fragment PostFields on Post { user { ...UserFields } }`
//!   - `post.graphql`: defines `query Post { post { ...PostFields } }`
//!
//! `UserFields` is used by `PostFields`; `PostFields` is used by the `Post`
//! query. All fragments are reachable from the root operation, so no errors.

use super::harness::Case;
use crate::rules::no_unused_fragments::NoUnusedFragmentsRuleImpl;

/// Content of `packages/plugin/__tests__/mocks/user-fields.graphql`
const USER_FIELDS: &str = "fragment UserFields on User {\n  id\n  firstName\n}";

/// Content of `packages/plugin/__tests__/mocks/post-fields.graphql`
const POST_FIELDS: &str = "fragment PostFields on Post {\n  user {\n    ...UserFields\n  }\n}";

/// Content of `packages/plugin/__tests__/mocks/post.graphql`
const POST: &str = "query Post {\n  post {\n    ...PostFields\n  }\n}";

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/__tests__/no-unused-fragments.spec.ts#L7>
#[test]
fn valid_l7_fragment_used_transitively_across_files() {
    // `user-fields.graphql` is the `code:` (primary) file. The other two
    // documents are supplied as extras so the rule can resolve all usages
    // before declaring a fragment unused.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/__tests__/no-unused-fragments.spec.ts#L7",
        super::UPSTREAM_SHA,
    ))
    .code(USER_FIELDS)
    .document("post-fields.graphql", POST_FIELDS)
    .document("post.graphql", POST)
    .run_against_project_document(NoUnusedFragmentsRuleImpl);
}
