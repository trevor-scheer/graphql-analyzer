// Flat-config presets — each one mirrors the contents of the corresponding
// `@graphql-eslint/eslint-plugin` preset so a user can rename the plugin
// scope and keep their preset reference unchanged. Spec-validation rules
// referenced here resolve to no-op stubs (`STUB_RULES` in `rules.ts`); the
// underlying check still runs as built-in validation. `naming-convention`
// uses options upstream supplies for these presets — features still being
// implemented on our side (PARITY_TODO item 3) silently degrade rather
// than hard-fail.

type RuleEntry = string | [string, ...unknown[]];

export const schemaRecommended: Record<string, RuleEntry> = {
  "@graphql-analyzer/description-style": "error",
  "@graphql-analyzer/known-argument-names": "error",
  "@graphql-analyzer/known-directives": "error",
  "@graphql-analyzer/known-type-names": "error",
  "@graphql-analyzer/lone-schema-definition": "error",
  "@graphql-analyzer/naming-convention": [
    "error",
    {
      types: "PascalCase",
      FieldDefinition: "camelCase",
      InputValueDefinition: "camelCase",
      Argument: "camelCase",
      DirectiveDefinition: "camelCase",
      EnumValueDefinition: "UPPER_CASE",
      "FieldDefinition[parent.name.value=Query]": {
        forbiddenPrefixes: ["query", "get"],
        forbiddenSuffixes: ["Query"],
      },
      "FieldDefinition[parent.name.value=Mutation]": {
        forbiddenPrefixes: ["mutation"],
        forbiddenSuffixes: ["Mutation"],
      },
      "FieldDefinition[parent.name.value=Subscription]": {
        forbiddenPrefixes: ["subscription"],
        forbiddenSuffixes: ["Subscription"],
      },
      "EnumTypeDefinition,EnumTypeExtension": {
        forbiddenPrefixes: ["Enum"],
        forbiddenSuffixes: ["Enum"],
      },
      "InterfaceTypeDefinition,InterfaceTypeExtension": {
        forbiddenPrefixes: ["Interface"],
        forbiddenSuffixes: ["Interface"],
      },
      "UnionTypeDefinition,UnionTypeExtension": {
        forbiddenPrefixes: ["Union"],
        forbiddenSuffixes: ["Union"],
      },
      "ObjectTypeDefinition,ObjectTypeExtension": {
        forbiddenPrefixes: ["Type"],
        forbiddenSuffixes: ["Type"],
      },
    },
  ],
  "@graphql-analyzer/no-hashtag-description": "error",
  "@graphql-analyzer/no-typename-prefix": "error",
  "@graphql-analyzer/no-unreachable-types": "error",
  "@graphql-analyzer/possible-type-extension": "error",
  "@graphql-analyzer/provided-required-arguments": "error",
  "@graphql-analyzer/require-deprecation-reason": "error",
  "@graphql-analyzer/require-description": [
    "error",
    { types: true, DirectiveDefinition: true, rootField: true },
  ],
  "@graphql-analyzer/strict-id-in-types": "error",
  "@graphql-analyzer/unique-directive-names": "error",
  "@graphql-analyzer/unique-directive-names-per-location": "error",
  "@graphql-analyzer/unique-enum-value-names": "error",
  "@graphql-analyzer/unique-field-definition-names": "error",
  "@graphql-analyzer/unique-operation-types": "error",
  "@graphql-analyzer/unique-type-names": "error",
};

// `flat/schema-all` extends recommended with the rest of our schema rules
// at error severity. Mirrors upstream's preset.
export const schemaAll: Record<string, RuleEntry> = {
  ...schemaRecommended,
  "@graphql-analyzer/alphabetize": [
    "error",
    {
      definitions: true,
      fields: ["ObjectTypeDefinition", "InterfaceTypeDefinition", "InputObjectTypeDefinition"],
      values: true,
      arguments: ["FieldDefinition", "Field", "DirectiveDefinition", "Directive"],
      groups: ["id", "*", "createdAt", "updatedAt"],
    },
  ],
  "@graphql-analyzer/input-name": "error",
  "@graphql-analyzer/no-root-type": ["error", { disallow: ["mutation", "subscription"] }],
  "@graphql-analyzer/no-scalar-result-type-on-mutation": "error",
  "@graphql-analyzer/require-deprecation-date": "error",
  "@graphql-analyzer/require-field-of-type-query-in-mutation-result": "error",
  "@graphql-analyzer/require-nullable-fields-with-oneof": "error",
  "@graphql-analyzer/require-nullable-result-in-root": "error",
  "@graphql-analyzer/require-type-pattern-with-oneof": "error",
};

// Mirrors `@graphql-eslint/eslint-plugin`'s `flat/schema-relay`.
export const schemaRelay: Record<string, RuleEntry> = {
  "@graphql-analyzer/relay-arguments": "error",
  "@graphql-analyzer/relay-connection-types": "error",
  "@graphql-analyzer/relay-edge-types": "error",
  "@graphql-analyzer/relay-page-info": "error",
};

export const operationsRecommended: Record<string, RuleEntry> = {
  "@graphql-analyzer/executable-definitions": "error",
  "@graphql-analyzer/fields-on-correct-type": "error",
  "@graphql-analyzer/fragments-on-composite-type": "error",
  "@graphql-analyzer/known-argument-names": "error",
  "@graphql-analyzer/known-directives": "error",
  "@graphql-analyzer/known-fragment-names": "error",
  "@graphql-analyzer/known-type-names": "error",
  "@graphql-analyzer/lone-anonymous-operation": "error",
  "@graphql-analyzer/naming-convention": [
    "error",
    {
      VariableDefinition: "camelCase",
      OperationDefinition: {
        style: "PascalCase",
        forbiddenPrefixes: ["Query", "Mutation", "Subscription", "Get"],
        forbiddenSuffixes: ["Query", "Mutation", "Subscription"],
      },
      FragmentDefinition: {
        style: "PascalCase",
        forbiddenPrefixes: ["Fragment"],
        forbiddenSuffixes: ["Fragment"],
      },
    },
  ],
  "@graphql-analyzer/no-anonymous-operations": "error",
  "@graphql-analyzer/no-deprecated": "error",
  "@graphql-analyzer/no-duplicate-fields": "error",
  "@graphql-analyzer/no-fragment-cycles": "error",
  "@graphql-analyzer/no-undefined-variables": "error",
  "@graphql-analyzer/no-unused-fragments": "error",
  "@graphql-analyzer/no-unused-variables": "error",
  "@graphql-analyzer/one-field-subscriptions": "error",
  "@graphql-analyzer/overlapping-fields-can-be-merged": "error",
  "@graphql-analyzer/possible-fragment-spread": "error",
  "@graphql-analyzer/provided-required-arguments": "error",
  "@graphql-analyzer/require-selections": "error",
  "@graphql-analyzer/scalar-leafs": "error",
  "@graphql-analyzer/selection-set-depth": ["error", { maxDepth: 7 }],
  "@graphql-analyzer/unique-argument-names": "error",
  "@graphql-analyzer/unique-directive-names-per-location": "error",
  "@graphql-analyzer/unique-fragment-name": "error",
  "@graphql-analyzer/unique-input-field-names": "error",
  "@graphql-analyzer/unique-operation-name": "error",
  "@graphql-analyzer/unique-variable-names": "error",
  "@graphql-analyzer/value-literals-of-correct-type": "error",
  "@graphql-analyzer/variables-are-input-types": "error",
  "@graphql-analyzer/variables-in-allowed-position": "error",
};

export const operationsAll: Record<string, RuleEntry> = {
  ...operationsRecommended,
  "@graphql-analyzer/alphabetize": [
    "error",
    {
      definitions: true,
      selections: ["OperationDefinition", "FragmentDefinition"],
      variables: true,
      arguments: ["Field", "Directive"],
      groups: ["...", "id", "*", "{"],
    },
  ],
  "@graphql-analyzer/lone-executable-definition": "error",
  "@graphql-analyzer/match-document-filename": [
    "error",
    {
      query: "kebab-case",
      mutation: "kebab-case",
      subscription: "kebab-case",
      fragment: "kebab-case",
    },
  ],
  "@graphql-analyzer/no-one-place-fragments": "error",
  "@graphql-analyzer/require-import-fragment": "error",
};

export const configs: Record<string, { rules: Record<string, RuleEntry> }> = {
  "flat/schema-recommended": { rules: schemaRecommended },
  "flat/schema-all": { rules: schemaAll },
  "flat/schema-relay": { rules: schemaRelay },
  "flat/operations-recommended": { rules: operationsRecommended },
  "flat/operations-all": { rules: operationsAll },
};
