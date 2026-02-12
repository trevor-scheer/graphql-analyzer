---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: minor
---

Support schema definitions in TypeScript/JavaScript files ([#561](https://github.com/trevor-scheer/graphql-analyzer/pull/561))

Schema files configured via `.graphqlrc.yaml` can now be TypeScript or JavaScript files containing GraphQL schema definitions in tagged template literals (e.g. `gql\`type User { ... }\``). Diagnostics, linting, and validation all report correct line/column positions within the original TS/JS file.
