export const schemaRecommended: Record<string, string> = {
  "@graphql-analyzer/description-style": "warn",
  "@graphql-analyzer/naming-convention": "error",
  "@graphql-analyzer/no-hashtag-description": "warn",
  "@graphql-analyzer/no-typename-prefix": "warn",
  "@graphql-analyzer/no-unreachable-types": "warn",
  "@graphql-analyzer/require-deprecation-reason": "warn",
  "@graphql-analyzer/require-description": "warn",
  "@graphql-analyzer/strict-id-in-types": "warn",
  "@graphql-analyzer/unique-enum-value-names": "warn",
};

export const operationsRecommended: Record<string, string> = {
  "@graphql-analyzer/no-anonymous-operations": "error",
  "@graphql-analyzer/no-deprecated": "warn",
  "@graphql-analyzer/no-duplicate-fields": "error",
  "@graphql-analyzer/unused-variables": "warn",
  "@graphql-analyzer/unused-fragments": "warn",
};

export const configs: Record<string, { rules: Record<string, string> }> = {
  "flat/schema-recommended": { rules: schemaRecommended },
  "flat/operations-recommended": { rules: operationsRecommended },
};
