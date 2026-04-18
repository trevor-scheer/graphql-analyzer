import graphqlPlugin from "@graphql-eslint/eslint-plugin";

export default [
  {
    files: ["**/*.graphql"],
    languageOptions: {
      parser: graphqlPlugin.parser,
    },
    plugins: {
      "@graphql-eslint": graphqlPlugin,
    },
    rules: {
      "@graphql-eslint/no-anonymous-operations": "error",
      "@graphql-eslint/no-duplicate-fields": "error",
      "@graphql-eslint/no-hashtag-description": "warn",
    },
  },
];
