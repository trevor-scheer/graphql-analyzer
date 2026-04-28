//! Verbatim port of `@graphql-eslint`'s `no-unused-fields` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts>

use super::harness::{Case, ExpectedError, ExpectedSuggestion};
use crate::rules::no_unused_fields::NoUnusedFieldsRuleImpl;

/// Full schema used by valid L58 and by the big valid case to establish that
/// all fields are reachable when the right operations are provided.
const TEST_SCHEMA: &str = r#"
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
"#;

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

// DIVERGENCE: upstream valid L101 uses `ignoredFieldSelectors` (Relay
// pagination field selectors) which we do not implement. The Relay schema
// has `PageInfo`, `*Edge`, and `*Connection` types whose fields
// (`hasPreviousPage`, `hasNextPage`, `startCursor`, `endCursor`, `cursor`,
// `pageInfo`) are present in the schema but not queried. Upstream suppresses
// those reports via the option; we would report them. Skipping rather than
// asserting divergent output because the option is entirely unimplemented and
// porting a valid case that would fail isn't useful.

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
    .code(r#"
        type User {
          id: ID!
          firstName: String
        }

        type Query {
          user(id: ID!): User
        }
      "#)
    .document(
        "op.graphql",
        r#"
            {
              user(id: 1) {
                id
              }
            }
        "#,
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

// DIVERGENCE: upstream invalid L134 expects `deleteUser` (a `Mutation` root
// field) to be reported as unused. Our rule intentionally skips root operation
// types (`Query`, `Mutation`, `Subscription`) on the grounds that callers
// choose which entry-points to invoke and there is no canonical "all
// operations must be called" requirement. Upstream imposes no such exclusion.
// We assert zero diagnostics here (our rule is silent on root-type fields).
/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unused-fields/index.test.ts#L134>
#[test]
fn invalid_l134_deleteuser_unused_root_field_divergence() {
    // DIVERGENCE: upstream reports Field "deleteUser" is unused (Mutation root
    // field). We skip all root-type fields; this case produces no diagnostics.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unused-fields/index.test.ts#L134",
        super::UPSTREAM_SHA,
    ))
    .code(r#"
        type Query {
          user(id: ID!): User
        }

        type Mutation {
          deleteUser(id: ID!): User
        }
      "#)
    .document(
        "op.graphql",
        r#"
            {
              user(id: 1) {
                id
              }
            }
        "#,
    )
    .run_against_project_schema(NoUnusedFieldsRuleImpl);
}
