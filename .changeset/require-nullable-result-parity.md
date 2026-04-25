---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-mcp: patch
graphql-analyzer-core: patch
graphql-analyzer-eslint-plugin: patch
---

`require-nullable-result-in-root` now matches `@graphql-eslint/eslint-plugin` exactly: non-null list types like `[User!]!` are no longer flagged (only non-null *named* returns are), and the diagnostic message is `Unexpected non-null result <type> in type "<root>"` to match graphql-eslint's wording.
