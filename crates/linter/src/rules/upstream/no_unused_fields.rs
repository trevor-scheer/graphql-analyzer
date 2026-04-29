//! Verbatim port of `@graphql-eslint`'s `no-unused-fields` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts>

use super::harness::{Case, ExpectedError, ExpectedSuggestion};
use crate::rules::no_unused_fields::NoUnusedFieldsRuleImpl;
use serde_json::json;

/// Full schema used by valid L58 and by the big valid case to establish that
/// all fields are reachable when the right operations are provided.
const TEST_SCHEMA: &str = r"
  type User {
    id: ID!
    firstName: String
    lastName: String
    age: Int
    address: Address
  }

  type Address {
    country: String!
    zip: String!
    events: [Event!]!
  }

  enum EventName {
    CREATE
    UPDATE
    DELETE
  }

  type Event {
    by: User
    name: EventName
    data: String
  }

  type Query {
    user(id: ID!): User
  }

  type Mutation {
    createUser(firstName: String!): User
    deleteUser(id: ID!): User
  }
";

/// Relay pagination schema — mirrors the `RELAY_SCHEMA` constant in upstream's
/// rule source and the example used in valid L101.
const RELAY_SCHEMA: &str = r"
  type Query {
    user: User
  }

  type User {
    id: ID!
    name: String!
    friends(first: Int, after: String): FriendConnection!
  }

  type FriendConnection {
    edges: [FriendEdge]
    pageInfo: PageInfo!
  }

  type FriendEdge {
    cursor: String!
    node: Friend!
  }

  type Friend {
    id: ID!
    name: String!
  }

  type PageInfo {
    hasPreviousPage: Boolean!
    hasNextPage: Boolean!
    startCursor: String
    endCursor: String
  }
";

/// Relay query — mirrors the `RELAY_QUERY` constant in upstream's rule source.
const RELAY_QUERY: &str = r"
  query {
    user {
      id
      name
      friends(first: 10) {
        edges {
          node {
            id
            name
          }
        }
      }
    }
  }
";

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts#L58>
#[test]
fn valid_l58_all_fields_used() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unused-fields/index.test.ts#L58",
        super::UPSTREAM_SHA,
    ))
    .code(TEST_SCHEMA)
    .document(
        "op.graphql",
        r#"
            {
              user(id: 1) {
                ... on User {
                  address {
                    zip
                    events {
                      ... on Event {
                        by {
                          id
                        }
                        can_rename: name
                        data
                      }
                    }
                  }
                }
              }
            }

            fragment UserFields on User {
              can_rename: firstName
              lastName
            }

            mutation {
              deleteUser(id: 2) {
                age
              }
              createUser(firstName: "Foo") {
                address {
                  country
                }
              }
            }
        "#,
    )
    .run_against_project_schema(NoUnusedFieldsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts#L101>
#[test]
fn valid_l101_relay_ignored_field_selectors() {
    // Relay pagination fields (PageInfo, *Edge cursor, *Connection pageInfo)
    // are present in the schema but not queried. Upstream suppresses them via
    // ignoredFieldSelectors; we pass the same selectors as options here.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unused-fields/index.test.ts#L101",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "ignoredFieldSelectors": [
            "[parent.name.value=PageInfo][name.value=/(endCursor|startCursor|hasNextPage|hasPreviousPage)/]",
            "[parent.name.value=/Edge$/][name.value=cursor]",
            "[parent.name.value=/Connection$/][name.value=pageInfo]",
        ]
    }))
    .code(RELAY_SCHEMA)
    .document("op.graphql", RELAY_QUERY)
    .run_against_project_schema(NoUnusedFieldsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts#L114>
#[test]
fn invalid_l114_firstname_unused() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unused-fields/index.test.ts#L114",
        super::UPSTREAM_SHA,
    ))
    // Upstream provides only `type User { ... }` as `code:` but the rule
    // tester has the full SCHEMA (including Query) as parserOptions context,
    // which is what makes the operation `{ user { id } }` resolve `User.id`
    // as used. Without a Query root the operation is un-typed and `id` appears
    // unused too. We add a minimal Query here to give the same resolution
    // context upstream has implicitly.
    .code(r"
        type User {
          id: ID!
          firstName: String
        }

        type Query {
          user(id: ID!): User
        }
      ")
    .document(
        "op.graphql",
        r"
            {
              user(id: 1) {
                id
              }
            }
        ",
    )
    .errors(vec![ExpectedError::new()
        .message("Field \"firstName\" is unused")
        .message_id("no-unused-fields")
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove `firstName` field",
            // Upstream doesn't pin suggestion output; we document ours here.
            "",
        )])])
    .run_against_project_schema(NoUnusedFieldsRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts#L134>
#[test]
fn invalid_l134_deleteuser_unused_root_field() {
    // Upstream reports `deleteUser` (a Mutation root field) as unused.
    // Root types are skipped by default (`skipRootTypes: true`); we opt in
    // to root-type checking here via `skipRootTypes: false` for upstream parity.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unused-fields/index.test.ts#L134",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "skipRootTypes": false }))
    .code(r"
        type Query {
          user(id: ID!): User
        }

        type Mutation {
          deleteUser(id: ID!): User
        }
      ")
    .document(
        "op.graphql",
        r"
            {
              user(id: 1) {
                id
              }
            }
        ",
    )
    .errors(vec![ExpectedError::new()
        .message("Field \"deleteUser\" is unused")
        .message_id("no-unused-fields")
        .suggestions(vec![ExpectedSuggestion::new(
            "Remove `deleteUser` field",
            "",
        )])])
    .run_against_project_schema(NoUnusedFieldsRuleImpl);
}
