import graphqlPlugin from "@graphql-analyzer/eslint-plugin";

export default [
  {
    files: ["**/*.graphql"],
    languageOptions: {
      parser: graphqlPlugin.parser,
    },
    plugins: {
      "@graphql-analyzer": graphqlPlugin,
    },
    rules: {
      "@graphql-analyzer/no-anonymous-operations": "error",
      "@graphql-analyzer/no-duplicate-fields": "error",
      "@graphql-analyzer/no-hashtag-description": "warn",
    },
  },
];
