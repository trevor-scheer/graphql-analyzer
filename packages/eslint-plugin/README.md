# @graphql-analyzer/eslint-plugin

Run native [graphql-analyzer] rules and JavaScript GraphQL custom rules in
ESLint. The default parser supports the [GraphQL-ESLint][graphql-eslint]
custom-rule API, including GraphQL visitors, `rawNode()`, `typeInfo()`, schema
services, and sibling operations.

Built-in rules use the Rust analyzer. Custom JavaScript rules run in ESLint.
Some built-in configuration and validation-rule differences still require
changes when migrating from GraphQL-ESLint.

## Install

Install the plugin and its GraphQL peer dependency:

```sh
npm install --save-dev @graphql-analyzer/eslint-plugin@alpha graphql@^16.5.0
```

Requires Node.js 18+ and ESLint 8.40+ or 9.x with flat config. npm installs the
native addon for your platform through optional dependencies.

## Use native rules

Enable native rules in your GraphQL project configuration:

```yaml
# .graphqlrc.yaml
schema: schema.graphql
documents: "src/**/*.graphql"
extensions:
  graphql-analyzer:
    lint:
      rules:
        noAnonymousOperations: error
```

Select the corresponding ESLint rules:

```js
// eslint.config.mjs
import graphql from "@graphql-analyzer/eslint-plugin";

export default [
  {
    files: ["**/*.graphql"],
    languageOptions: { parser: graphql.parser },
    plugins: { "@graphql-analyzer": graphql },
    rules: { "@graphql-analyzer/no-anonymous-operations": "error" },
  },
];
```

Native rule options belong in `.graphqlrc.yaml`; ESLint rule options are not
forwarded to Rust. The native engine supports one project per config.

## Add custom rules

Register custom rules through ESLint's `plugins` configuration and keep
`graphql.parser`. Import `GraphQLESLintRule`, `requireGraphQLSchema`, and
`requireGraphQLOperations` from this package when writing typed or
schema-aware rules. Use ESLint's ordinary `RuleTester` with the same parser.

For embedded GraphQL, add `processor: graphql.processor` to your host-language
configuration. Keep that language's parser and rules. The processor exposes
virtual `.graphql` documents, so the GraphQL config above also applies to
embedded custom rules. Reports, suggestions, and safe fixes map back to the
original file.

## Choose native-only mode

Use `graphql.fastParser` for standalone GraphQL files when all rules are
native. Use `graphql.fastProcessor` for native embedded checks, with the
host-language parser. These entry points avoid loading the JavaScript
compatibility frontend. They do not provide GraphQL node visitors or parser
services to custom rules.

The compatible parser adds JavaScript parsing and ESLint traversal. Its cost
depends on the workload; ESLint performance is not equivalent to CLI performance.

See the [ESLint plugin documentation] for complete configuration, custom-rule
examples, supported embedded forms, and migration limitations.

## License

MIT OR Apache-2.0

[graphql-analyzer]: https://github.com/trevor-scheer/graphql-analyzer
[graphql-eslint]: https://the-guild.dev/graphql/eslint/docs
[ESLint plugin documentation]: https://trevor-scheer.github.io/graphql-analyzer/linting/eslint-plugin/
