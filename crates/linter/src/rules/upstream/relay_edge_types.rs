//! Verbatim port of `@graphql-eslint`'s `relay-edge-types` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::relay_edge_types::RelayEdgeTypesRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L16>
#[test]
fn valid_l16_cursor_returns_string_should_implement_node_false() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L16",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type AEdge {
          node: Int!
          cursor: String!
        }
        type AConnection {
          edges: [AEdge]
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": false }))
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L29>
#[test]
fn valid_l29_cursor_returns_scalar() {
    // A custom `Email` scalar is valid for both `node` and `cursor`.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L29",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        scalar Email
        type AEdge {
          node: Email!
          cursor: Email!
        }
        type AConnection {
          edges: [AEdge]
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": false }))
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L43>
#[test]
fn valid_l43_with_edge_suffix_true() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L43",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        scalar Email
        type AEdge {
          node: Email!
          cursor: Email!
        }
        type AConnection {
          edges: [AEdge]
        }
      "#,
    )
    .options(serde_json::json!({ "withEdgeSuffix": true, "shouldImplementNode": false }))
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L57>
#[test]
fn valid_l57_should_implement_node() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L57",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        interface Node {
          id: ID!
        }
        type User implements Node {
          id: ID!
        }
        type AEdge {
          node: User!
          cursor: String!
        }
        type AConnection {
          edges: [AEdge]
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": true }))
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L74>
#[test]
fn valid_l74_not_object_type_no_throw() {
    // `Int` is a built-in scalar so it is not in schema_types; the
    // `shouldImplementNode` path only fires for Object/Interface types found
    // in the schema map, so this is valid.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L74",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type AEdge {
          node: Int!
          cursor: String!
        }
        type AConnection {
          edges: [AEdge]
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": false }))
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L89>
#[test]
fn invalid_l89_edge_type_must_be_object_type() {
    // `scalar AEdge`, `union BEdge`, `enum CEdge`, `interface DEdge` are all
    // referenced as edge types but are not Object types → 4 errors.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L89",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type PageInfo
        type BConnection
        type DConnection
        scalar AEdge
        union BEdge = PageInfo
        enum CEdge
        interface DEdge
        type AConnection {
          edges: [AEdge]
          pageInfo: PageInfo!
        }
        extend type BConnection {
          edges: [BEdge!]
          pageInfo: PageInfo!
        }
        type CConnection {
          edges: [CEdge]!
          pageInfo: PageInfo!
        }
        extend type DConnection {
          edges: [DEdge!]!
          pageInfo: PageInfo!
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": false, "listTypeCanWrapOnlyEdgeType": false }))
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L118>
#[test]
fn invalid_l118_fields_missing() {
    // `AEdge` has no fields → 2 errors (missing `node` and `cursor`).
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L118",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type PageInfo
        type AEdge
        type AConnection {
          edges: [AEdge]
          pageInfo: PageInfo!
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": false }))
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L131>
#[test]
fn invalid_l131_cursor_list_type() {
    // Both `node` and `cursor` return `[PageInfo!]!` — a list type is invalid
    // for both → 2 errors.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L131",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type PageInfo
        type AEdge {
          node: [PageInfo!]!
          cursor: [PageInfo!]!
        }
        type AConnection {
          edges: [AEdge]
          pageInfo: PageInfo!
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": false, "listTypeCanWrapOnlyEdgeType": false }))
    .errors(vec![ExpectedError::new(), ExpectedError::new()])
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L149>
#[test]
fn invalid_l149_without_edge_suffix() {
    // `Aedge` (lowercase 'e') does not end with "Edge" → 1 error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L149",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        scalar Email
        type Aedge {
          node: Email!
          cursor: Email!
        }
        type AConnection {
          edges: [Aedge]
        }
      "#,
    )
    .options(serde_json::json!({ "withEdgeSuffix": true, "shouldImplementNode": false }))
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L163>
#[test]
fn invalid_l163_list_type_can_wrap_only_edge_type() {
    // `listTypeCanWrapOnlyEdgeType: true` — all list fields that don't wrap an
    // edge type are flagged. `User.comments/likes/messages/posts` all wrap
    // `Int`, which is not an edge type → 4 errors.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L163",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type AEdge {
          node: Int!
          cursor: String!
        }
        type AConnection {
          edges: [AEdge]
        }
        type User {
          comments: [Int]
          likes: [Int!]
          messages: [Int]!
          posts: [Int!]!
        }
      "#,
    )
    .options(serde_json::json!({ "listTypeCanWrapOnlyEdgeType": true, "shouldImplementNode": false }))
    .errors(vec![
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
        ExpectedError::new(),
    ])
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/relay-edge-types/index.test.ts#L184>
#[test]
fn invalid_l184_should_implement_node() {
    // `User` does not implement `Node` → 1 error.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/relay-edge-types/index.test.ts#L184",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type User {
          id: ID!
        }
        type AEdge {
          node: User!
          cursor: String!
        }
        type AConnection {
          edges: [AEdge]
        }
      "#,
    )
    .options(serde_json::json!({ "shouldImplementNode": true }))
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RelayEdgeTypesRuleImpl);
}
