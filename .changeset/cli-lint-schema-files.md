---
graphql-analyzer-cli: patch
---

Run schema-only lint rules (`noUnreachableTypes`, etc.) from `graphql lint`, `graphql check`, and `graphql fix`. Previously the CLI walked document files only, so these rules silently dropped diagnostics on schema files even when configured ([#1074](https://github.com/trevor-scheer/graphql-analyzer/pull/1074))
