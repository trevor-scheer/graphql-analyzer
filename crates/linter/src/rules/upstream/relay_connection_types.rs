//! Verbatim port of `@graphql-eslint`'s `relay-connection-types` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::relay_connection_types::RelayConnectionTypesRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L8>
#[test]
fn valid_l8_follow_relay_spec() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L8",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type UserConnection {
          edges: [UserEdge]
          pageInfo: PageInfo!
        }
      ",
    )
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L14>
#[test]
fn valid_l14_edges_returns_list_wrapping_edge_type() {
    // Various non-null wrapper combinations on `edges` are all valid so long
    // as the field is a list type. `pageInfo` must be non-null `PageInfo!`.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L14",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type UserConnection {
          edges: [UserEdge]
          pageInfo: PageInfo!
        }
        type PostConnection {
          edges: [PostEdge!]
          pageInfo: PageInfo!
        }
        type CommentConnection {
          edges: [CommentEdge]!
          pageInfo: PageInfo!
        }
        type AddressConnection {
          edges: [AddressEdge!]!
          pageInfo: PageInfo!
        }
      ",
    )
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L33>
#[test]
fn valid_l33_unnamed_string() {
    // Unnamed code-only form (no `name:` property).
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
      type UserConnection {
        edges: [UserEdge]
        pageInfo: PageInfo!
      }
    ",
    )
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L40>
#[test]
fn valid_l40_extend_type() {
    // `extend type` that is a valid Connection is also acceptable.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L40",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
      extend type UserConnection {
        edges: [UserEdge]
        pageInfo: PageInfo!
      }
    ",
    )
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L50>
#[test]
fn invalid_l50_non_object_types_with_connection_suffix() {
    // Non-Object types (scalar, union, input, enum, interface) whose names end
    // in `Connection` all violate the rule.
    //
    // DIVERGENCE: upstream counts `errors: 9` because it treats each `extend`
    // as a separate violation (1 per base + 1 per extension = 9). Our HIR
    // merges type extensions into the base type, so we see 5 merged types and
    // produce 5 errors.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L50",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        scalar DateTimeConnection
        union DataConnection = Post
        extend union DataConnection = Comment
        input CreateUserConnection
        extend input CreateUserConnection {
          firstName: String
        }
        enum RoleConnection
        extend enum RoleConnection {
          ADMIN
        }
        interface NodeConnection
        extend interface NodeConnection {
          id: ID!
        }
        type Post
        type Comment
      ",
    )
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L73>
#[test]
fn invalid_l73_missing_connection_suffix() {
    // Object type with both `edges` and `pageInfo` but no `Connection` suffix.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L73",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type User {
          edges: UserEdge
          pageInfo: PageInfo
        }
      ",
    )
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L82>
#[test]
fn invalid_l82_missing_edges_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L82",
        super::UPSTREAM_SHA,
    ))
    .code("type UserConnection { pageInfo: PageInfo! }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L87>
#[test]
fn invalid_l87_missing_page_info_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L87",
        super::UPSTREAM_SHA,
    ))
    .code("type UserConnection { edges: [UserEdge] }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L92>
#[test]
fn invalid_l92_edges_not_list_type() {
    // `edges` field returns a named type rather than a list — 2 errors (one per
    // connection type).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L92",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type AConnection {
          edges: AEdge
          pageInfo: PageInfo!
        }
        type BConnection {
          edges: BEdge!
          pageInfo: PageInfo!
        }
      ",
    )
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-connection-types/index.test.ts#L104>
#[test]
fn invalid_l104_page_info_not_non_null_page_info() {
    // Various invalid `pageInfo` return types: nullable, list-wrapped, etc.
    // 5 connection types, each with a bad `pageInfo`.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-connection-types/index.test.ts#L104",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type AConnection {
          edges: [AEdge]
          pageInfo: PageInfo
        }
        type BConnection {
          edges: [BEdge]
          pageInfo: [PageInfo]
        }
        type CConnection {
          edges: [CEdge]
          pageInfo: [PageInfo!]
        }
        type DConnection {
          edges: [DEdge]
          pageInfo: [PageInfo]!
        }
        type EConnection {
          edges: [EEdge]
          pageInfo: [PageInfo!]!
        }
      ",
    )
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RelayConnectionTypesRuleImpl);
}
