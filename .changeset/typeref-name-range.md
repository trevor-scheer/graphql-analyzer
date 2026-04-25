---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-mcp: patch
graphql-analyzer-core: patch
graphql-analyzer-eslint-plugin: patch
---

Type-position diagnostics for `require-field-of-type-query-in-mutation-result` and `require-nullable-result-in-root` now report at the field's return type name node, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1008](https://github.com/trevor-scheer/graphql-analyzer/pull/1008)).
