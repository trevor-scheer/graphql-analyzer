# @graphql-lsp/build-plugins

GraphQL validation and linting plugins for popular build tools: Vite, Webpack, and esbuild.

## Features

- Validate GraphQL files during build
- Run lint rules with configurable severity
- Fail builds on errors/warnings (configurable)
- Watch mode support with hot reload feedback
- Native performance via Rust-based validation

## Installation

```bash
npm install @graphql-lsp/build-plugins @graphql-lsp/node
```

## Usage

### Vite

```ts
// vite.config.ts
import { defineConfig } from 'vite';
import { graphqlPlugin } from '@graphql-lsp/build-plugins/vite';

export default defineConfig({
  plugins: [
    graphqlPlugin({
      schema: './schema.graphql',
      failOnError: true,
      lint: {
        'no_deprecated': 'warn',
      },
    }),
  ],
});
```

### Webpack

```js
// webpack.config.js
const { GraphQLLspPlugin } = require('@graphql-lsp/build-plugins/webpack');

module.exports = {
  plugins: [
    new GraphQLLspPlugin({
      schema: './schema.graphql',
      failOnError: true,
      lint: {
        'no_deprecated': 'warn',
      },
    }),
  ],
};
```

### esbuild

```js
// esbuild.config.js
const { graphqlPlugin } = require('@graphql-lsp/build-plugins/esbuild');

require('esbuild').build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  plugins: [
    graphqlPlugin({
      schema: './schema.graphql',
      failOnError: true,
      lint: {
        'no_deprecated': 'warn',
      },
    }),
  ],
});
```

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `schema` | `string` | (required) | Path to GraphQL schema file or inline SDL |
| `include` | `string[]` | `["**/*.graphql", "**/*.gql"]` | Glob patterns for files to validate |
| `exclude` | `string[]` | `["node_modules/**"]` | Glob patterns for files to exclude |
| `failOnError` | `boolean` | `true` | Fail build on validation errors |
| `failOnWarning` | `boolean` | `false` | Fail build on lint warnings |
| `lint` | `Record<string, "error" \| "warn" \| "off">` | `{}` | Lint rule configuration |
| `verbose` | `boolean` | `false` | Enable verbose logging |

## Lint Rules

Configure lint rules using the `lint` option:

```ts
graphqlPlugin({
  schema: './schema.graphql',
  lint: {
    'no_deprecated': 'warn',      // Warn on deprecated usage
    'unique_names': 'error',      // Error on duplicate names
    'require_id_field': 'off',    // Disable this rule
  },
});
```

## Inline Schema

You can provide the schema inline:

```ts
graphqlPlugin({
  schema: `
    type Query {
      hello: String
    }
  `,
});
```

## Watch Mode

All plugins support watch mode:

- **Vite**: Diagnostics shown in terminal during `vite dev`
- **Webpack**: Diagnostics shown during `webpack --watch`
- **esbuild**: Use esbuild's watch API with the plugin

## Error Output

Validation errors appear in your build output:

```
ERROR [no_deprecated]: Field 'User.legacyId' is deprecated
  at src/queries/users.graphql:5:3

WARNING [unique_names]: Operation 'GetUser' is defined multiple times
  at src/queries/users.graphql:1:1

1 error(s), 1 warning(s)
```

## License

MIT OR Apache-2.0
