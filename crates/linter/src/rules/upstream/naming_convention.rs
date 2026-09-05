//! Verbatim port of `@graphql-eslint`'s `naming-convention` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts>

use serde_json::json;

use super::harness::{Case, ExpectedError};
use crate::rules::naming_convention::NamingConventionRuleImpl;

// ---------------------------------------------------------------------------
// valid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L6>
#[test]
fn valid_l6_operation_fragment_variables() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L6",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "types": "PascalCase",
        "VariableDefinition": "camelCase",
        "EnumValueDefinition": "UPPER_CASE",
        "OperationDefinition": "PascalCase",
        "FragmentDefinition": "PascalCase",
    }))
    .code(
        "query GetUser($userId: ID!) {\n  user(id: $userId) {\n    id\n    name\n    isViewerFriend\n    profilePicture(size: 50) {\n      ...PictureFragment\n    }\n  }\n}\n\nfragment PictureFragment on Picture {\n  uri\n  width\n  height\n}\n",
    )
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L36>
#[test]
fn valid_l36_types_pascal_case_b() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L36",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase" }))
    .code("type B { test: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L37>
#[test]
fn valid_l37_types_snake_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L37",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "snake_case" }))
    .code("type my_test_6_t { test: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L38>
#[test]
fn valid_l38_types_upper_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L38",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "UPPER_CASE" }))
    .code("type MY_TEST_6_T { test: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L39>
#[test]
fn valid_l39_no_underscores_config() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L39",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "allowLeadingUnderscore": false, "allowTrailingUnderscore": false }))
    .code("type B { test: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L40>
#[test]
fn valid_l40_allow_leading_trailing_underscore() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L40",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "allowLeadingUnderscore": true,
        "allowTrailingUnderscore": true,
        "types": "PascalCase",
        "FieldDefinition": "camelCase",
    }))
    .code("type __B { __test__: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L48>
#[test]
fn valid_l48_scalar_pascal_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L48",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase" }))
    .code("scalar BSONDecimal")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L49>
#[test]
fn valid_l49_interface_pascal_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L49",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase" }))
    .code("interface B { test: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L50>
#[test]
fn valid_l50_enum_upper_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L50",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase", "EnumValueDefinition": "UPPER_CASE" }))
    .code("enum B { TEST }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L51>
#[test]
fn valid_l51_input_camel_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L51",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase", "InputValueDefinition": "camelCase" }))
    .code("input Test { item: String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L52>
/// name: (bare string case — no options)
#[test]
fn valid_l52_no_options_bare_string() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L52",
        super::UPSTREAM_SHA,
    ))
    .code("input test { item: String } enum B { Test } interface A { i: String } fragment PictureFragment on Picture { uri } scalar Hello")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L53>
#[test]
fn valid_l53_object_form_with_suffix() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L53",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "types": { "style": "PascalCase" },
        "FieldDefinition": { "style": "camelCase", "suffix": "Field" },
        "EnumValueDefinition": { "style": "UPPER_CASE", "suffix": "" },
    }))
    .code("type TypeOne { aField: String } enum Z { VALUE_ONE VALUE_TWO }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L65>
#[test]
fn valid_l65_object_form_with_prefix() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L65",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "types": { "style": "PascalCase" },
        "FieldDefinition": { "style": "camelCase", "prefix": "field" },
        "EnumValueDefinition": { "style": "UPPER_CASE", "prefix": "ENUM_VALUE_" },
    }))
    .code("type One { fieldA: String } enum Z { ENUM_VALUE_ONE ENUM_VALUE_TWO }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L76>
#[test]
fn valid_l76_selector_parent_predicate() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L76",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[parent.name.value=Query]": { "style": "UPPER_CASE", "prefix": "QUERY" },
        "FieldDefinition[parent.name.value!=Query]": { "style": "camelCase", "prefix": "field" },
    }))
    .code("type One { fieldA: String } type Query { QUERY_A(id: ID!): String }")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L85>
#[test]
fn valid_l85_operation_definition_pascal_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L85",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "OperationDefinition": { "style": "PascalCase" } }))
    .code("query { foo }")
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L86>
/// name: no-options bare code (includes `__typename` and `ok_`)
#[test]
fn valid_l86_typename_introspection_no_options() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L86",
        super::UPSTREAM_SHA,
    ))
    .code("{ test { __typename ok_ } }")
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L87>
/// name: 'should ignore fields'
#[test]
fn valid_l87_should_ignore_fields() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L87",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition": {
            "style": "camelCase",
            "ignorePattern": "^(EU|IE|GB|UK|CC|UPC|CMWA|EAN13)",
        },
    }))
    .code(
        "type Test {\n  EU: ID\n  EUIntlFlag: ID\n  IE: ID\n  IEIntlFlag: ID\n  GB: ID\n  UKFlag: ID\n  UKService_Badge: ID\n  CCBleaching: ID\n  CCDryCleaning: ID\n  CCIroning: ID\n  UPC: ID\n  CMWA: ID\n  EAN13: ID\n}\n",
    )
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L109>
/// name: 'should allow single letter for camelCase'
#[test]
fn valid_l109_single_letter_camel_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L109",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "ObjectTypeDefinition": "camelCase" }))
    .code("type t")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L113>
/// name: 'should allow single letter for `PascalCase`'
#[test]
fn valid_l113_single_letter_pascal_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L113",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "ObjectTypeDefinition": "PascalCase" }))
    .code("type T")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L117>
/// name: 'should allow single letter for `snake_case`'
#[test]
fn valid_l117_single_letter_snake_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L117",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "ObjectTypeDefinition": "snake_case" }))
    .code("type t")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L121>
/// name: 'should allow single letter for `UPPER_CASE`'
#[test]
fn valid_l121_single_letter_upper_case() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L121",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "ObjectTypeDefinition": "UPPER_CASE" }))
    .code("type T")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L125>
/// requiredPrefixes with type-based selectors
#[test]
fn valid_l125_required_prefixes_type_selectors() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L125",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[gqlType.gqlType.name.value=Boolean]": {
            "style": "camelCase",
            "requiredPrefixes": ["is", "has"],
        },
        "FieldDefinition[gqlType.gqlType.name.value=Secret]": {
            "requiredPrefixes": ["SUPER_SECRET_"],
        },
        "FieldDefinition[gqlType.name.value=Snake]": {
            "style": "snake_case",
            "requiredPrefixes": ["hiss_"],
        },
    }))
    .code(
        "scalar Secret\n\ninterface Snake {\n  value: String!\n}\n\ntype Test {\n  isEnabled: Boolean!\n  SUPER_SECRET_secret: Secret!\n  hiss_snake: Snake\n}\n",
    )
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L148>
/// requiredSuffixes with type-based selectors
#[test]
fn valid_l148_required_suffixes_type_selectors() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L148",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[gqlType.gqlType.name.value=Boolean]": {
            "style": "camelCase",
            "requiredSuffixes": ["Enabled", "Disabled"],
        },
        "FieldDefinition[gqlType.gqlType.name.value=IpAddress]": {
            "requiredSuffixes": ["IpAddress"],
        },
    }))
    .code(
        "scalar IpAddress\n\ntype Test {\n  specialFeatureEnabled: Boolean!\n  userIpAddress: IpAddress!\n}\n",
    )
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L163>
/// name: 'should not fail when aliasing underscore fields'
#[test]
fn valid_l163_alias_underscore_fields() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L163",
        super::UPSTREAM_SHA,
    ))
    .code("{\n  test {\n    bar: __foo\n    foo: bar__\n  }\n}\n")
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L172>
/// name: '`allowLeadingUnderscore` and `allowTrailingUnderscore` should not conflict with `ignorePattern`'
#[test]
fn valid_l172_allow_underscore_with_ignore_pattern() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L172",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[parent.name.value=SomeType]": {
            "ignorePattern": ".*someField.*",
        },
    }))
    .code("type SomeType {\n  _someField_: Boolean!\n}\n")
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L183>
/// name: 'requiredPattern with case style in prefix'
/// Upstream uses a JS `RegExp` `/^(?<camelCase>.+?)_/`; we use the string form.
#[test]
fn valid_l183_required_pattern_case_style_in_prefix() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L183",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FragmentDefinition": {
            "style": "PascalCase",
            // Upstream uses JS RegExp /^(?<camelCase>.+?)_/; equivalent string form:
            "requiredPattern": "^(?<camelCase>.+?)_",
        },
    }))
    .code("fragment myUser_UserProfileFields on User {\n  id\n}\n")
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L197>
/// name: 'requiredPattern with case style in suffix'
/// Upstream uses JS `RegExp` `/_(?<snake_case>.+?)$/`; we use the string form.
#[test]
fn valid_l197_required_pattern_case_style_in_suffix() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L197",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FragmentDefinition": {
            "style": "PascalCase",
            // Upstream uses JS RegExp /_(?<snake_case>.+?)$/; equivalent string form:
            "requiredPattern": "_(?<snake_case>.+?)$",
        },
    }))
    .code("fragment UserProfileFields_my_user on User {\n  id\n}\n")
    .run_against_standalone_document(NamingConventionRuleImpl);
}

// ---------------------------------------------------------------------------
// invalid
// ---------------------------------------------------------------------------

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L214>
#[test]
fn invalid_l214_type_and_field_pascal_case() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L214",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase", "FieldDefinition": "PascalCase" }))
    .code("type b { test: String }")
    .errors(vec![
        ExpectedError::new().message(r#"Type "b" should be in PascalCase format"#),
        ExpectedError::new().message(r#"Field "test" should be in PascalCase format"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L222>
#[test]
fn invalid_l222_leading_trailing_underscores_not_allowed() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L222",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "allowLeadingUnderscore": false, "allowTrailingUnderscore": false }))
    .code("type __b { test__: String }")
    .errors(vec![
        ExpectedError::new().message("Leading underscores are not allowed"),
        ExpectedError::new().message("Trailing underscores are not allowed"),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L230>
#[test]
fn invalid_l230_scalar_snake_case() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L230",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "ScalarTypeDefinition": "snake_case" }))
    .code("scalar BSONDecimal")
    .errors(vec![
        ExpectedError::new().message(r#"Scalar "BSONDecimal" should be in snake_case format"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

// The `large.graphql` mock-file test (upstream line ~234) is omitted because
// the `ruleTester.fromMockFile('large.graphql')` fixture is not available
// in this repo. It tests 27 errors against a large generated schema.

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L275>
#[test]
fn invalid_l275_enum_camel_case_values_upper_case() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L275",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "EnumTypeDefinition": "camelCase",
        "EnumValueDefinition": "UPPER_CASE",
    }))
    .code("enum B { test }")
    .errors(vec![
        ExpectedError::new().message(r#"Enum "B" should be in camelCase format"#),
        ExpectedError::new().message(r#"Enum value "test" should be in UPPER_CASE format"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L284>
#[test]
fn invalid_l284_input_pascal_case_value_snake_case() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L284",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "types": "PascalCase", "InputValueDefinition": "snake_case" }))
    .code("input test { _Value: String }")
    .errors(vec![
        ExpectedError::new().message(r#"Input "test" should be in PascalCase format"#),
        ExpectedError::new().message(r#"Input value "_Value" should be in snake_case format"#),
        ExpectedError::new().message("Leading underscores are not allowed"),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L293>
#[test]
fn invalid_l293_field_suffix_enum_suffix() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L293",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "ObjectTypeDefinition": { "style": "camelCase" },
        "FieldDefinition": { "style": "camelCase", "suffix": "AAA" },
        "EnumValueDefinition": { "style": "camelCase", "suffix": "ENUM" },
    }))
    .code("type TypeOne { aField: String } enum Z { VALUE_ONE VALUE_TWO }")
    .errors(vec![
        ExpectedError::new().message(r#"Type "TypeOne" should be in camelCase format"#),
        ExpectedError::new().message(r#"Field "aField" should have "AAA" suffix"#),
        ExpectedError::new().message(r#"Enum value "VALUE_ONE" should have "ENUM" suffix"#),
        ExpectedError::new().message(r#"Enum value "VALUE_TWO" should have "ENUM" suffix"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L306>
#[test]
fn invalid_l306_field_prefix_enum_prefix() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L306",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "ObjectTypeDefinition": { "style": "PascalCase" },
        "FieldDefinition": { "style": "camelCase", "prefix": "Field" },
        "EnumValueDefinition": { "style": "UPPER_CASE", "prefix": "ENUM" },
    }))
    .code("type One { aField: String } enum Z { A_ENUM_VALUE_ONE VALUE_TWO }")
    .errors(vec![
        ExpectedError::new().message(r#"Field "aField" should have "Field" prefix"#),
        ExpectedError::new().message(r#"Enum value "A_ENUM_VALUE_ONE" should have "ENUM" prefix"#),
        ExpectedError::new().message(r#"Enum value "VALUE_TWO" should have "ENUM" prefix"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L319>
#[test]
fn invalid_l319_forbidden_prefixes_and_suffixes() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L319",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "ObjectTypeDefinition": { "style": "PascalCase", "forbiddenPrefixes": ["On"] },
        "FieldDefinition": {
            "style": "camelCase",
            "forbiddenPrefixes": ["foo", "bar"],
            "forbiddenSuffixes": ["Foo"],
        },
        "FieldDefinition[parent.name.value=Query]": {
            "style": "camelCase",
            "forbiddenPrefixes": ["get", "query"],
        },
    }))
    .code("type One { getFoo: String, queryBar: String } type Query { getA(id: ID!): String, queryB: String } extend type Query { getC: String }")
    .errors(vec![
        ExpectedError::new().message(r#"Type "One" should not have "On" prefix"#),
        ExpectedError::new().message(r#"Field "getFoo" should not have "Foo" suffix"#),
        ExpectedError::new().message(r#"Field "getA" should not have "get" prefix"#),
        ExpectedError::new().message(r#"Field "queryB" should not have "query" prefix"#),
        ExpectedError::new().message(r#"Field "getC" should not have "get" prefix"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L337>
#[test]
fn invalid_l337_operation_camel_case_forbidden_prefix() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L337",
        super::UPSTREAM_SHA,
    ))
    .options(json!({ "OperationDefinition": { "style": "camelCase", "forbiddenPrefixes": ["get"] } }))
    .code("query Foo { foo } query getBar { bar }")
    .errors(vec![
        ExpectedError::new().message(r#"Query "Foo" should be in camelCase format"#),
        ExpectedError::new().message(r#"Query "getBar" should not have "get" prefix"#),
    ])
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L346>
/// name: 'schema-recommended config'
/// Upstream pins `errors: 15` (count only). We document our errors here.
/// The schema-recommended config uses JS `RegExp` patterns; we substitute
/// equivalent string-source regex forms. This is not a divergence —
/// it's the documented deserialization shim.
#[test]
fn invalid_l346_schema_recommended_config() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L346",
        super::UPSTREAM_SHA,
    ))
    .options(json!([{
        "types": "PascalCase",
        "FieldDefinition": "camelCase",
        "InputValueDefinition": "camelCase",
        "Argument": "camelCase",
        "DirectiveDefinition": "camelCase",
        "EnumValueDefinition": "UPPER_CASE",
        "FieldDefinition[parent.name.value=Query]": {
            // Upstream uses JS RegExp /^(query|get)/i and /query$/i
            "forbiddenPatterns": ["(?i)^(query|get)", "(?i)query$"],
        },
        "FieldDefinition[parent.name.value=Mutation]": {
            // Upstream uses JS RegExp /(^mutation)|(mutation$)/i
            "forbiddenPatterns": ["(?i)(^mutation)|(mutation$)"],
        },
        "FieldDefinition[parent.name.value=Subscription]": {
            // Upstream uses JS RegExp /(^subscription)|(subscription$)/i
            "forbiddenPatterns": ["(?i)(^subscription)|(subscription$)"],
        },
        "EnumTypeDefinition,EnumTypeExtension": {
            // Upstream uses JS RegExp /(^enum)|(enum$)/i
            "forbiddenPatterns": ["(?i)(^enum)|(enum$)"],
        },
        "InterfaceTypeDefinition,InterfaceTypeExtension": {
            // Upstream uses JS RegExp /(^interface)|(interface$)/i
            "forbiddenPatterns": ["(?i)(^interface)|(interface$)"],
        },
        "UnionTypeDefinition,UnionTypeExtension": {
            // Upstream uses JS RegExp /(^union)|(union$)/i
            "forbiddenPatterns": ["(?i)(^union)|(union$)"],
        },
        "ObjectTypeDefinition,ObjectTypeExtension": {
            // Upstream uses JS RegExp /(^type)|(type$)/i
            "forbiddenPatterns": ["(?i)(^type)|(type$)"],
        },
    }]))
    .code(
        "type Query {\n  fieldQuery: ID\n  queryField: ID\n  getField: ID\n}\n\ntype Mutation {\n  fieldMutation: ID\n  mutationField: ID\n}\n\ntype Subscription {\n  fieldSubscription: ID\n  subscriptionField: ID\n}\n\nenum TestEnum\nextend enum EnumTest {\n  A\n}\n\ninterface TestInterface\nextend interface InterfaceTest {\n  id: ID\n}\n\nunion TestUnion\nextend union UnionTest = TestInterface\n\ntype TestType\nextend type TypeTest {\n  id: ID\n}\n",
    )
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        // Query fields with forbidden prefixes/suffixes
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        // Mutation fields
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        // Subscription fields
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        // Enum names with forbidden patterns
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        // Interface names
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        // Union names
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
        // Object type names
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L404>
/// name: 'operations-recommended config'
/// JS `RegExp` patterns substituted with equivalent string-source regex forms.
#[test]
fn invalid_l404_operations_recommended_config() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L404",
        super::UPSTREAM_SHA,
    ))
    .options(json!([{
        "VariableDefinition": "camelCase",
        "OperationDefinition": {
            "style": "PascalCase",
            // Upstream: [/^(query|mutation|subscription|get)/i, /(query|mutation|subscription)$/i]
            "forbiddenPatterns": [
                "(?i)^(query|mutation|subscription|get)",
                "(?i)(query|mutation|subscription)$",
            ],
        },
        "FragmentDefinition": {
            "style": "PascalCase",
            // Upstream: [/(^fragment)|(fragment$)/i]
            "forbiddenPatterns": ["(?i)(^fragment)|(fragment$)"],
        },
    }]))
    .code(
        "query TestQuery { test }\nquery QueryTest { test }\nquery GetQuery { test }\nquery Test { test(CONTROLLED_BY_SCHEMA: 0) }\n\nmutation TestMutation { test }\nmutation MutationTest { test }\n\nsubscription TestSubscription { test }\nsubscription SubscriptionTest { test }\n\nfragment TestFragment on Test { id }\nfragment FragmentTest on Test { id }\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"Query "TestQuery" should not contain the forbidden pattern "/(query|mutation|subscription)$/i""#),
        ExpectedError::new().message(r#"Query "QueryTest" should not contain the forbidden pattern "/^(query|mutation|subscription|get)/i""#),
        ExpectedError::new().message(r#"Query "GetQuery" should not contain the forbidden pattern "/^(query|mutation|subscription|get)/i""#),
        ExpectedError::new().message(r#"Mutation "TestMutation" should not contain the forbidden pattern "/(query|mutation|subscription)$/i""#),
        ExpectedError::new().message(r#"Mutation "MutationTest" should not contain the forbidden pattern "/^(query|mutation|subscription|get)/i""#),
        ExpectedError::new().message(r#"Subscription "TestSubscription" should not contain the forbidden pattern "/(query|mutation|subscription)$/i""#),
        ExpectedError::new().message(r#"Subscription "SubscriptionTest" should not contain the forbidden pattern "/^(query|mutation|subscription|get)/i""#),
        ExpectedError::new().message(r#"Fragment "TestFragment" should not contain the forbidden pattern "/(^fragment)|(fragment$)/i""#),
        ExpectedError::new().message(r#"Fragment "FragmentTest" should not contain the forbidden pattern "/(^fragment)|(fragment$)/i""#),
    ])
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L430>
/// name: 'should ignore selections fields but check alias renaming'
#[test]
fn invalid_l430_alias_leading_trailing_underscore() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L430",
        super::UPSTREAM_SHA,
    ))
    .code(
        "{\n  test {\n    _badAlias: foo\n    badAlias_: bar\n    _ok\n    ok_\n  }\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message("Leading underscores are not allowed"),
        ExpectedError::new().message("Trailing underscores are not allowed"),
    ])
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L444>
/// name: 'should error when selected type names do not match require prefixes'
/// Upstream pins `errors: 3` (count only); we document our errors here.
#[test]
fn invalid_l444_required_prefixes_type_selectors_errors() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L444",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[gqlType.gqlType.name.value=Boolean]": {
            "style": "camelCase",
            "requiredPrefixes": ["is", "has"],
        },
        "FieldDefinition[gqlType.gqlType.name.value=Secret]": {
            "requiredPrefixes": ["SUPER_SECRET_"],
        },
        "FieldDefinition[gqlType.name.value=Snake]": {
            "style": "snake_case",
            "requiredPrefixes": ["hiss"],
        },
    }))
    .code(
        "scalar Secret\n\ninterface Snake {\n  value: String!\n}\n\ntype Test {\n  enabled: Boolean!\n  secret: Secret!\n  snake: Snake\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"Field "enabled" should have one of the following prefixes: is or has"#),
        ExpectedError::new().message(r#"Field "secret" should have one of the following prefixes: SUPER_SECRET_"#),
        ExpectedError::new().message(r#"Field "snake" should have one of the following prefixes: hiss"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L469>
/// name: 'should error when selected type names do not match require suffixes'
/// Upstream pins `errors: 2` (count only); we document our errors here.
#[test]
fn invalid_l469_required_suffixes_type_selectors_errors() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L469",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[gqlType.gqlType.name.value=Boolean]": {
            "style": "camelCase",
            "requiredSuffixes": ["Enabled", "Disabled"],
        },
        "FieldDefinition[gqlType.gqlType.name.value=IpAddress]": {
            "requiredSuffixes": ["IpAddress"],
        },
    }))
    .code(
        "scalar IpAddress\n\ntype Test {\n  specialFeature: Boolean!\n  user: IpAddress!\n}\n",
    )
    .errors(vec![
        ExpectedError::new().message(r#"Field "specialFeature" should have one of the following suffixes: Enabled or Disabled"#),
        ExpectedError::new().message(r#"Field "user" should have one of the following suffixes: IpAddress"#),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L486>
/// name: 'forbiddenPatterns'
/// Upstream uses JS `RegExp` `/^(get|query)/`; we use string form.
/// Upstream pins `errors: 2` (count only); we document our errors here.
#[test]
fn invalid_l486_forbidden_patterns() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L486",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        // Upstream uses JS RegExp /^(get|query)/; equivalent string form:
        "OperationDefinition": { "forbiddenPatterns": ["^(get|query)"] },
    }))
    .code("query queryFoo { foo } query getBar { bar }")
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("namingConvention"),
        ExpectedError::new().message_id("namingConvention"),
    ])
    .run_against_standalone_document(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L492>
/// name: 'requiredPattern'
/// Upstream uses JS `RegExp` `/^(is|has)/`; we use string form.
/// Upstream pins `errors: 1` (count only); we document our error here.
#[test]
fn invalid_l492_required_pattern() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L492",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FieldDefinition[gqlType.gqlType.name.value=Boolean]": {
            "style": "camelCase",
            // Upstream uses JS RegExp /^(is|has)/; equivalent string form:
            "requiredPattern": "^(is|has)",
        },
    }))
    .code("type Test { enabled: Boolean! }")
    .errors(vec![
        ExpectedError::new().message_id("namingConvention"),
    ])
    .run_against_standalone_schema(NamingConventionRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/naming-convention/index.test.ts#L503>
/// name: 'requiredPattern with case style in suffix'
/// Upstream uses JS `RegExp` `/_(?<camelCase>.+?)$/`; we use string form.
/// Upstream pins `errors: 1` (count only); we document our error here.
#[test]
fn invalid_l503_required_pattern_with_case_style_in_suffix() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/naming-convention/index.test.ts#L503",
        super::UPSTREAM_SHA,
    ))
    .options(json!({
        "FragmentDefinition": {
            "style": "PascalCase",
            // Upstream uses JS RegExp /_(?<camelCase>.+?)$/; equivalent string form:
            "requiredPattern": "_(?<camelCase>.+?)$",
        },
    }))
    .code("fragment UserProfileFields on User {\n  id\n}\n")
    // Upstream doesn't pin individual messages; we document ours here.
    .errors(vec![
        ExpectedError::new().message_id("namingConvention"),
    ])
    .run_against_standalone_document(NamingConventionRuleImpl);
}
