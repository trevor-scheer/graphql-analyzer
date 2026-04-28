//! Verbatim port of `@graphql-eslint`'s `alphabetize` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts>

use serde_json::json;

use super::harness::{Case, ExpectedError};
use crate::rules::alphabetize::AlphabetizeRuleImpl;

// ---------------------------------------------------------------------------
// valid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L22>
#[test]
fn valid_l22_fields_object_type_definition() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L22",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["ObjectTypeDefinition"] }))
    .code(
        "        type User {\n          age: Int\n          firstName: String!\n          lastName: String!\n          password: String\n        }\n",
    )
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L33>
#[test]
fn valid_l33_fields_input_object_type_definition() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L33",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["InputObjectTypeDefinition"] }))
    .code(
        "        input UserInput {\n          age: Int\n          firstName: String!\n          lastName: String!\n          password: String\n          zip: String\n        }\n",
    )
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L45>
#[test]
fn valid_l45_values() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L45",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "values": true }))
    .code(
        "        enum Role {\n          ADMIN\n          GOD\n          SUPER_ADMIN\n          USER\n        }\n",
    )
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L56>
/// name: 'should not report error if selection is duplicated'
#[test]
fn valid_l56_no_error_if_selection_duplicated() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L56",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "selections": ["OperationDefinition"] }))
    .code(
        "        query {\n          test {\n            a\n            a\n            b\n          }\n        }\n",
    )
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

// ---------------------------------------------------------------------------
// invalid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L71>
#[test]
fn invalid_l71_fields_object_type_definition_unordered() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L71",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["ObjectTypeDefinition"] }))
    .code(
        "        type User {\n          password: String\n          firstName: String!\n          age: Int\n          lastName: String!\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"field "firstName" should be before field "password""#),
        ExpectedError::new().message(r#"field "age" should be before field "firstName""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L86>
#[test]
fn invalid_l86_fields_extend_type_unordered() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L86",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["ObjectTypeDefinition"] }))
    .code(
        "        extend type User {\n          age: Int\n          firstName: String!\n          password: String\n          lastName: String!\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"field "lastName" should be before field "password""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L98>
#[test]
fn invalid_l98_fields_interface_type_definition() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L98",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["InterfaceTypeDefinition"] }))
    .code(
        "        interface Test {\n          cc: Int\n          bb: Int\n          aa: Int\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"field "bb" should be before field "cc""#),
        ExpectedError::new().message(r#"field "aa" should be before field "bb""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L112>
#[test]
fn invalid_l112_fields_input_object_type_definition() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L112",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["InputObjectTypeDefinition"] }))
    .code(
        "        input UserInput {\n          password: String\n          firstName: String!\n          age: Int\n          lastName: String!\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"input value "firstName" should be before input value "password""#),
        ExpectedError::new().message(r#"input value "age" should be before input value "firstName""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L127>
#[test]
fn invalid_l127_fields_extend_input_unordered() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L127",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["InputObjectTypeDefinition"] }))
    .code(
        "        extend input UserInput {\n          age: Int\n          firstName: String!\n          password: String\n          lastName: String!\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"input value "lastName" should be before input value "password""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L139>
#[test]
fn invalid_l139_values_enum_unordered() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L139",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "values": true }))
    .code(
        "        enum Role {\n          SUPER_ADMIN\n          ADMIN\n          USER\n          GOD\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"enum value "ADMIN" should be before enum value "SUPER_ADMIN""#),
        ExpectedError::new().message(r#"enum value "GOD" should be before enum value "USER""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L154>
#[test]
fn invalid_l154_values_extend_enum_unordered() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L154",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "values": true }))
    .code(
        "        extend enum Role {\n          ADMIN\n          SUPER_ADMIN\n          GOD\n          USER\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"enum value "GOD" should be before enum value "SUPER_ADMIN""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L166>
#[test]
fn invalid_l166_arguments_directive_definition() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L166",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "arguments": ["DirectiveDefinition"] }))
    .code("        directive @test(cc: [Cc!]!, bb: [Bb!], aa: Aa!) on FIELD_DEFINITION\n")
    .errors(vec![
        ExpectedError::new().message(r#"input value "bb" should be before input value "cc""#),
        ExpectedError::new().message(r#"input value "aa" should be before input value "bb""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L176>
#[test]
fn invalid_l176_arguments_field_definition() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L176",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "arguments": ["FieldDefinition"] }))
    .code(
        "        type Query {\n          test(cc: [Cc!]!, bb: [Bb!], aa: Aa!): Int\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"input value "bb" should be before input value "cc""#),
        ExpectedError::new().message(r#"input value "aa" should be before input value "bb""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L188>
#[test]
fn invalid_l188_selections_fragment_definition() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L188",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "selections": ["FragmentDefinition"] }))
    .code(
        "        fragment TestFields on Test {\n          cc\n          bb\n          aa\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"field "bb" should be before field "cc""#),
        ExpectedError::new().message(r#"field "aa" should be before field "bb""#),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L202>
// DIVERGENCE (two differences from upstream):
//
// 1. Diagnostic order: our rule uses two passes over each selection set (first
//    the ordering scan, then recursion into nested sets). So the outer-level
//    `bb/aa` errors fire BEFORE the inner inline-fragment `bbb/aaa` errors,
//    while upstream processes the inner set first because it walks the ESTree
//    recursively in a single pass.
//
// 2. Message for `bb`: upstream tracks inline fragments as sentinels in the
//    ordering sequence and emits `field "bb" should be before inline fragment`.
//    Our rule skips inline fragments, so `last` remains `cc` and the message
//    reads `field "bb" should be before field "cc"`.
#[test]
fn invalid_l202_selections_operation_definition_with_inline_fragment() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L202",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "selections": ["OperationDefinition"] }))
    .code(
        "        query {\n          test {\n            cc\n            ... on Test {\n              ccc\n              bbb\n              aaa\n            }\n            bb\n            aa\n          }\n        }\n",
    )
    // DIVERGENCE: outer errors fire before inner errors (two-pass iteration), and
    // `bb` compares against `cc` (not `inline fragment`).
    .errors(vec![
        ExpectedError::new().message(r#"field "bb" should be before field "cc""#),
        ExpectedError::new().message(r#"field "aa" should be before field "bb""#),
        ExpectedError::new().message(r#"field "bbb" should be before field "ccc""#),
        ExpectedError::new().message(r#"field "aaa" should be before field "bbb""#),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L225>
#[test]
fn invalid_l225_variables_and_arguments_field() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L225",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "variables": true, "arguments": ["Field"] }))
    .code(
        "        mutation ($cc: [Cc!]!, $bb: [Bb!], $aa: Aa!) {\n          test(ccc: $cc, bbb: $bb, aaa: $aa) {\n            something\n          }\n        }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"variable "bb" should be before variable "cc""#),
        ExpectedError::new().message(r#"variable "aa" should be before variable "bb""#),
        ExpectedError::new().message(r#"argument "bbb" should be before argument "ccc""#),
        ExpectedError::new().message(r#"argument "aaa" should be before argument "bbb""#),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L241>
/// name: 'should move comment'
#[test]
fn invalid_l241_should_move_comment() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L241",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "fields": ["ObjectTypeDefinition"] }))
    .code(concat!(
        "        type Test { # { character\n",
        "          # before d 1\n",
        "\n",
        "          # before d 2\n",
        "          d: Int # same d\n",
        "          # before c\n",
        "          c: Float!\n",
        "          # before b 1\n",
        "          # before b 2\n",
        "          b: [String] # same b\n",
        "          # before a\n",
        "          a: [Int!]! # same a\n",
        "          # end\n",
        "        } # } character\n",
    ))
    .errors(vec![
        ExpectedError::new().message(r#"field "c" should be before field "d""#),
        ExpectedError::new().message(r#"field "b" should be before field "c""#),
        ExpectedError::new().message(r#"field "a" should be before field "b""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L266>
/// name: 'should compare with lexicographic order'
#[test]
fn invalid_l266_compare_with_lexicographic_order() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L266",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "values": true }))
    .code(concat!(
        "        enum Test {\n",
        "          \"qux\"\n",
        "          qux\n",
        "          foo\n",
        "          \"Bar\"\n",
        "          Bar\n",
        "          \"\"\"\n",
        "          bar\n",
        "          \"\"\"\n",
        "          bar\n",
        "        }\n",
    ))
    .errors(vec![
        ExpectedError::new().message(r#"enum value "foo" should be before enum value "qux""#),
        ExpectedError::new().message(r#"enum value "Bar" should be before enum value "foo""#),
        ExpectedError::new().message(r#"enum value "bar" should be before enum value "Bar""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L288>
/// name: 'should sort definitions'
#[test]
fn invalid_l288_should_sort_definitions() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L288",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "definitions": true }))
    .code(concat!(
        "        # START\n",
        "\n",
        "        # before1 extend union Data\n",
        "        # before2 extend union Data\n",
        "        extend union Data = Role # same extend union Data\n",
        "        # before extend input UserInput\n",
        "        extend input UserInput {\n",
        "          email: Email!\n",
        "        } # same extend input UserInput\n",
        "        # before fragment UserFields\n",
        "        fragment UserFields on User {\n",
        "          id\n",
        "        } # same fragment UserFields\n",
        "        # before type User\n",
        "        type User # same type User\n",
        "        # before extend enum Role\n",
        "        extend enum Role {\n",
        "          SUPERMAN\n",
        "        } # same extend enum Role\n",
        "        # before anonymous operation\n",
        "        query {\n",
        "          foo\n",
        "        } # same anonymous operation\n",
        "        # before mutation CreateUser\n",
        "        mutation CreateUser {\n",
        "          createUser\n",
        "        } # same mutation CreateUser\n",
        "        # before extend interface Node\n",
        "        extend interface Node {\n",
        "          createdAt: String!\n",
        "        } # same extend interface Node\n",
        "        # before extend interface Node\n",
        "        extend interface Node {\n",
        "          updatedAt: String!\n",
        "        } # same extend interface Node\n",
        "        # before type RootQuery\n",
        "        type RootQuery # same type RootQuery\n",
        "        # before interface Node\n",
        "        interface Node # same interface Node\n",
        "        # before enum Role\n",
        "        enum Role # same enum Role\n",
        "        # before scalar Email\n",
        "        scalar Email # same scalar Email\n",
        "        # before input UserInput\n",
        "        input UserInput # same input UserInput\n",
        "        # before extend type User\n",
        "        extend type User {\n",
        "          firstName: String!\n",
        "        } # same extend type User\n",
        "        # before schema definition\n",
        "        schema {\n",
        "          query: RootQuery\n",
        "        } # same schema definition\n",
        "        # before union Data\n",
        "        union Data = User | Node # same union Data\n",
        "        # before directive @auth\n",
        "        directive @auth(role: [Role!]!) on FIELD_DEFINITION # same directive @auth\n",
        "\n",
        "        # END\n",
    ))
    .errors(vec![
        ExpectedError::new().message(r#"fragment "UserFields" should be before input "UserInput""#),
        ExpectedError::new().message(r#"type "User" should be before fragment "UserFields""#),
        ExpectedError::new().message(r#"enum "Role" should be before type "User""#),
        ExpectedError::new().message(r#"mutation "CreateUser" should be before operation definition"#),
        ExpectedError::new().message(r#"interface "Node" should be before type "RootQuery""#),
        ExpectedError::new().message(r#"scalar "Email" should be before enum "Role""#),
        ExpectedError::new().message(r#"type "User" should be before input "UserInput""#),
        ExpectedError::new().message(r#"union "Data" should be before schema definition"#),
        ExpectedError::new().message(r#"directive "auth" should be before union "Data""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L364>
/// name: 'should sort when selection is aliased'
#[test]
fn invalid_l364_sort_when_selection_is_aliased() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L364",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "selections": ["OperationDefinition"] }))
    .code(concat!(
        "        {\n",
        "          # start\n",
        "          lastName: lastname # lastName comment\n",
        "          fullName: fullname # fullName comment\n",
        "          firsName: firstname # firsName comment\n",
        "          # end\n",
        "        }\n",
    ))
    .errors(vec![
        ExpectedError::new().message(r#"field "fullName" should be before field "lastName""#),
        ExpectedError::new().message(r#"field "firsName" should be before field "fullName""#),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

// GROUP_ORDER_TEST constant inlined from upstream (used in the next 3 cases):
//
//   type User {
//     firstName: Int
//     createdAt: DateTime
//     author: Int
//     wagon: Int
//     id: ID
//     foo: Int
//     updatedAt: DateTime
//     bar: Int
//     nachos: Int
//     guild: Int
//   }
const GROUP_ORDER_TEST: &str = concat!(
    "  type User {\n",
    "    firstName: Int\n",
    "    createdAt: DateTime\n",
    "    author: Int\n",
    "    wagon: Int\n",
    "    id: ID\n",
    "    foo: Int\n",
    "    updatedAt: DateTime\n",
    "    bar: Int\n",
    "    nachos: Int\n",
    "    guild: Int\n",
    "  }\n",
);

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L381>
/// name: 'should sort by group when `*` is between'
#[test]
fn invalid_l381_sort_by_group_star_is_between() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L381",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "fields": ["ObjectTypeDefinition"],
        "groups": ["id", "*", "createdAt", "updatedAt"],
    }))
    .code(GROUP_ORDER_TEST)
    .errors(vec![
        ExpectedError::new().message(r#"field "author" should be before field "createdAt""#),
        ExpectedError::new().message(r#"field "id" should be before field "wagon""#),
        ExpectedError::new().message(r#"field "bar" should be before field "updatedAt""#),
        ExpectedError::new().message(r#"field "guild" should be before field "nachos""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L397>
/// name: 'should sort by group when `*` is at the end'
#[test]
fn invalid_l397_sort_by_group_star_at_end() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L397",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "fields": ["ObjectTypeDefinition"],
        "groups": ["updatedAt", "id", "createdAt", "*"],
    }))
    .code(GROUP_ORDER_TEST)
    .errors(vec![
        ExpectedError::new().message(r#"field "createdAt" should be before field "firstName""#),
        ExpectedError::new().message(r#"field "id" should be before field "wagon""#),
        ExpectedError::new().message(r#"field "updatedAt" should be before field "foo""#),
        ExpectedError::new().message(r#"field "guild" should be before field "nachos""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L413>
/// name: 'should sort by group when `*` at the start'
#[test]
fn invalid_l413_sort_by_group_star_at_start() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L413",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "fields": ["ObjectTypeDefinition"],
        "groups": ["*", "updatedAt", "id", "createdAt"],
    }))
    .code(GROUP_ORDER_TEST)
    .errors(vec![
        ExpectedError::new().message(r#"field "author" should be before field "createdAt""#),
        ExpectedError::new().message(r#"field "foo" should be before field "id""#),
        ExpectedError::new().message(r#"field "bar" should be before field "updatedAt""#),
        ExpectedError::new().message(r#"field "guild" should be before field "nachos""#),
    ])
    .run_against_standalone_schema(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L429>
/// name: 'should sort selections by group when `*` is between'
/// Upstream uses `errors: 3` (count only); we document our errors here.
#[test]
fn invalid_l429_sort_selections_by_group_star_between() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L429",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "selections": ["OperationDefinition"],
        "groups": ["id", "*", "createdAt", "updatedAt"],
    }))
    .code(concat!(
        "        {\n",
        "          zz\n",
        "          updatedAt\n",
        "          createdAt\n",
        "          aa\n",
        "          id\n",
        "        }\n",
    ))
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L448>
/// name: 'should sort selections by group when `...` is at the start'
/// Upstream uses `errors: 4` (count only); we document our errors here.
#[test]
fn invalid_l448_sort_selections_by_group_spread_at_start() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L448",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "selections": ["OperationDefinition"],
        "groups": ["...", "id", "*", "createdAt", "updatedAt"],
    }))
    .code(concat!(
        "        {\n",
        "          zz\n",
        "          updatedAt\n",
        "          createdAt\n",
        "          aa\n",
        "          id\n",
        "          ...ChildFragment\n",
        "        }\n",
    ))
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L468>
/// name: 'should sort selections by group when `...` is between'
/// Upstream uses `errors: 3` (count only); we document our errors here.
#[test]
fn invalid_l468_sort_selections_by_group_spread_between() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L468",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "selections": ["FragmentDefinition"],
        "groups": ["id", "*", "...", "createdAt", "updatedAt"],
    }))
    .code(concat!(
        "        fragment foo on Foo {\n",
        "          zz\n",
        "          ...ChildFragment\n",
        "          updatedAt\n",
        "          createdAt\n",
        "          aa\n",
        "          id\n",
        "        }\n",
    ))
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L488>
/// name: 'should sort selections by group when `...` is at the end'
/// Upstream uses `errors: 4` (count only); we document our errors here.
#[test]
fn invalid_l488_sort_selections_by_group_spread_at_end() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L488",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "selections": ["OperationDefinition"],
        "groups": ["id", "*", "createdAt", "updatedAt", "..."],
    }))
    .code(concat!(
        "        {\n",
        "          ...ChildFragment\n",
        "          zz\n",
        "          updatedAt\n",
        "          createdAt\n",
        "          aa\n",
        "          id\n",
        "        }\n",
    ))
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L508>
/// name: 'should sort selection set at the end'
/// Upstream uses `errors: 2` (count only); we document our errors here.
#[test]
fn invalid_l508_sort_selection_set_at_end() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L508",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "selections": ["OperationDefinition"],
        "groups": ["id", "*", "updatedAt", "{"],
    }))
    .code(concat!(
        "        {\n",
        "          zz\n",
        "          updatedAt\n",
        "          createdAt {\n",
        "            __typename\n",
        "          }\n",
        "          aa\n",
        "          user {\n",
        "            id\n",
        "          }\n",
        "          aab {\n",
        "            id\n",
        "          }\n",
        "        }\n",
    ))
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/alphabetize/index.test.ts#L534>
/// name: 'should sort selection set at the start'
/// Upstream uses `errors: 3` (count only); we document our errors here.
#[test]
fn invalid_l534_sort_selection_set_at_start() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/alphabetize/index.test.ts#L534",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "selections": ["OperationDefinition"],
        "groups": ["{", "id", "*", "updatedAt"],
    }))
    .code(concat!(
        "        {\n",
        "          zz\n",
        "          updatedAt\n",
        "          createdAt {\n",
        "            __typename\n",
        "          }\n",
        "          aa\n",
        "          user {\n",
        "            id\n",
        "          }\n",
        "          aab {\n",
        "            id\n",
        "          }\n",
        "        }\n",
    ))
    // Upstream doesn't pin individual messages; we document our errors here.
    .errors(vec![
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
        ExpectedError::new().message_id("alphabetize"),
    ])
    .run_against_standalone_document(AlphabetizeRuleImpl);
}
