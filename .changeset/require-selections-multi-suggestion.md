---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-eslint-plugin: patch
---

`require-selections`: emit one quick-fix suggestion per missing `idName` instead of a single autofix that stacks every candidate. Picking which `idName` to add is a semantic choice; the IDE menu now offers one entry per candidate, matching `@graphql-eslint`. The single-candidate case still autofixes ([#1079](https://github.com/trevor-scheer/graphql-analyzer/pull/1079))
