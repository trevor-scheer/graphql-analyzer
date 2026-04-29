# @graphql-analyzer/eslint-plugin

ESLint plugin for GraphQL, powered by [graphql-analyzer]. A drop-in replacement
for [`@graphql-eslint/eslint-plugin`][graphql-eslint] — same plugin name, same
rule names, same flat-config preset names. The Rust analyzer does the work via
a native addon, so performance matches the CLI.

## Install

```sh
npm install --save-dev @graphql-analyzer/eslint-plugin@alpha
```

Requires Node.js 18+ and ESLint 8.40+ or 9.x (flat config only). The native
addon ships as platform-specific optional dependencies; npm picks the right
one for your machine automatically.

## Usage

```js
// eslint.config.mjs
import graphql from "@graphql-analyzer/eslint-plugin";

export default [
  {
    files: ["**/*.graphql"],
    languageOptions: { parser: graphql.parser },
    plugins: { "@graphql-analyzer": graphql },
    rules: graphql.configs["flat/operations-recommended"].rules,
  },
];
```

For embedded GraphQL in TypeScript/JavaScript files, the migration guide, the
full rule reference, and configuration via `.graphqlrc.yaml`, see the docs:

→ <https://trevor-scheer.github.io/graphql-analyzer/linting/eslint-plugin/>

## License

MIT OR Apache-2.0

[graphql-analyzer]: https://github.com/trevor-scheer/graphql-analyzer
[graphql-eslint]: https://the-guild.dev/graphql/eslint/docs
