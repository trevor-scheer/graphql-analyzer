---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
---

Fix unused fragment auto-fix in TS/JS files to delete the entire variable declaration instead of just the GraphQL content ([#487](https://github.com/trevor-scheer/graphql-analyzer/issues/487))
