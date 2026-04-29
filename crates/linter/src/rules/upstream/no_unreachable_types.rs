//! Verbatim port of `@graphql-eslint`'s `no-unreachable-types` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::no_unreachable_types::NoUnreachableTypesRuleImpl;

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L8>
#[test]
fn valid_l8_union_of_scalars() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L8",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        scalar A
        scalar B

        # UnionTypeDefinition
        union Response = A | B

        type Query {
          foo: Response
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L23>
#[test]
fn valid_l23_object_type_reachable() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L23",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type Query {
          me: User
        }

        # ObjectTypeDefinition
        type User {
          id: ID
          name: String
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L38>
#[test]
fn valid_l38_interface_type_reachable() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L38",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type Query {
          me: User
        }

        # InterfaceTypeDefinition
        interface Address {
          city: String
        }

        type User implements Address {
          city: String
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L56>
#[test]
fn valid_l56_scalar_reachable() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L56",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        # ScalarTypeDefinition
        scalar DateTime

        type Query {
          now: DateTime
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L67>
#[test]
fn valid_l67_enum_reachable() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L67",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        # EnumTypeDefinition
        enum Role {
          ADMIN
          USER
        }

        type Query {
          role: Role
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L81>
#[test]
fn valid_l81_input_type_in_argument() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L81",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        input UserInput {
          id: ID
        }

        type Query {
          # InputValueDefinition
          user(input: UserInput!): Boolean
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L83>
#[test]
fn valid_l83_directive_arg_type_reachable_via_field_usage() {
    // `Role` is only referenced as an argument type of `@auth`, which is applied
    // to a reachable field. Upstream marks arg types of applied directives
    // reachable when it visits `Directive` nodes during the AST walk.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L83",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        # DirectiveDefinition
        directive @auth(role: [Role!]!) on FIELD_DEFINITION

        enum Role {
          ADMIN
          USER
        }

        type Query {
          # Directive
          user: ID @auth(role: [ADMIN])
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L112>
#[test]
fn valid_l112_custom_root_types_all_reachable() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L112",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type RootQuery
        type RootMutation
        type RootSubscription

        schema {
          query: RootQuery
          mutation: RootMutation
          subscription: RootSubscription
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L124>
#[test]
fn valid_l124_interface_implementing_interface() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L124",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        interface User {
          id: ID!
        }

        interface Manager implements User {
          id: ID!
        }

        type TopManager implements Manager {
          id: ID!
          name: String
        }

        type Query {
          me: User
        }
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L132>
#[test]
fn valid_l132_directive_on_schema_reachable() {
    // `@good` is applied on the schema definition. Upstream marks it reachable
    // via the `Directive` visitor when walking the schema AST node. We don't
    // report directives (only named types), so this passes vacuously: `Query`
    // is the only named type and it is the root, so no errors.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L132",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type Query

        schema @good {
          query: Query
        }

        directive @good on SCHEMA
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L144>
#[test]
fn valid_l144_directives_with_request_locations_ignored() {
    // Directives with request-side locations are not named types, so our rule
    // never reports them. The only named type here is `Query` (root). No errors.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L144",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        directive @q on QUERY
        directive @w on MUTATION
        directive @e on SUBSCRIPTION
        directive @r on FIELD
        directive @t on FRAGMENT_DEFINITION
        directive @y on FRAGMENT_SPREAD
        directive @u on INLINE_FRAGMENT
        directive @i on VARIABLE_DEFINITION
        type Query
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L158>
#[test]
fn valid_l158_enum_in_directive_arg_with_request_location() {
    // `Enum` is only used as an argument type of `@q`, which has location QUERY
    // (executable). Upstream's request-location pass marks arg types of such
    // directives reachable. `type Query` (no fields) is valid SDL.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L158",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        enum Enum {
          A
          B
        }
        directive @q(arg: Enum = A) on QUERY
        type Query
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L169>
#[test]
fn valid_l169_scalars_in_directive_args_with_request_locations() {
    // Scalars used only as directive argument types are reachable when the
    // directive has an executable location. Upstream's request-location pass
    // marks them reachable without needing a path from the root.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L169",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        scalar Scalar1
        scalar Scalar2
        scalar Scalar3
        scalar Scalar4
        scalar Scalar5
        scalar Scalar6
        directive @q(arg: Scalar1) on QUERY
        directive @w(arg: Scalar2!) on QUERY
        directive @e(arg: [Scalar3]) on QUERY
        directive @r(arg: [Scalar4!]) on QUERY
        directive @t(arg: [Scalar5]!) on QUERY
        directive @y(arg: [Scalar6!]!) on QUERY
      ",
    )
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L188>
#[test]
fn invalid_l188_interface_and_object_unreachable() {
    // Upstream: `AnotherNode` is the return type of `Query.node`; `Node`,
    // `User`, and `SuperUser` are not reachable from any root.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L188",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type Query {
          node(id: ID!): AnotherNode!
        }

        interface Node {
          id: ID!
        }

        interface AnotherNode {
          createdAt: String
        }

        interface User implements Node {
          id: ID!
          name: String
        }

        type SuperUser implements User & Node {
          id: ID!
          name: String
          address: String
        }
      ",
    )
    .errors(vec![
        ExpectedError::new().message("Interface type `Node` is unreachable."),
        ExpectedError::new().message("Interface type `User` is unreachable."),
        ExpectedError::new().message("Object type `SuperUser` is unreachable."),
    ])
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

// DIVERGENCE: upstream invalid L219 expects `Directive \`auth\` is
// unreachable.` for an unused directive definition among other unreachable
// types. Our rule does not report unreachable directive definitions — only
// named types (object, interface, union, enum, input). The case would fail
// both on count (we'd emit 6, upstream expects 7) and the missing directive
// message. Skipping rather than asserting partial output.

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L261>
#[test]
fn invalid_l261_scalar_unreachable() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L261",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        interface User {
          id: String
        }

        type SuperUser implements User {
          id: String
          superDetail: SuperDetail
        }

        type SuperDetail {
          detail: String
        }

        type Query {
          user: User!
        }

        scalar DateTime
      ",
    )
    .errors(vec![ExpectedError::new()
        .message("Scalar type `DateTime` is unreachable.")])
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L284>
#[test]
fn invalid_l284_type_extension_unreachable() {
    // Upstream emits 3 errors: `User` (unreachable interface), `SuperUser`
    // (base definition), and `SuperUser` (extension — each AST node is checked
    // separately).  We now also fire on extension declarations, matching upstream.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L284",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        interface User {
          id: String
        }

        interface AnotherUser {
          createdAt: String
        }

        type SuperUser implements User {
          id: String
        }

        # ObjectTypeExtension
        extend type SuperUser {
          detail: String
        }

        type Query {
          user: AnotherUser!
        }
      ",
    )
    .errors(vec![
        ExpectedError::new().message("Interface type `User` is unreachable."),
        ExpectedError::new().message("Object type `SuperUser` is unreachable."),
        ExpectedError::new().message("Object type `SuperUser` is unreachable."),
    ])
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L313>
#[test]
fn invalid_l313_scalar_unreachable_with_reachable_chain() {
    // Node/User/SuperUser are all reachable through the `Node` interface
    // return type; only `DateTime` is unreachable.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/no-unreachable-types/index.test.ts#L313",
        super::UPSTREAM_SHA,
    ))
    .code(
        r"
        type Query {
          node(id: ID!): Node!
        }

        interface Node {
          id: ID!
        }

        interface User implements Node {
          id: ID!
          name: String
        }

        type SuperUser implements User & Node {
          id: ID!
          name: String
          address: String
        }

        scalar DateTime
      ",
    )
    .errors(vec![ExpectedError::new()
        .message("Scalar type `DateTime` is unreachable.")])
    .run_against_standalone_schema(NoUnreachableTypesRuleImpl);
}
