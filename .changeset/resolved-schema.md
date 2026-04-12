---
graphql-analyzer-cli: major
graphql-analyzer-lsp: major
graphql-analyzer-vscode: major
---

Namespace extensions under `extensions.graphql-analyzer` and add resolved schema support ([#966](https://github.com/trevor-scheer/graphql-analyzer/pull/966))

BREAKING: `client`, `lint`, and `extractConfig` must now be nested under `extensions.graphql-analyzer` in `.graphqlrc.yaml`.

New: `resolvedSchema` config option to validate queries against a build-generated schema while keeping source files for navigation.
