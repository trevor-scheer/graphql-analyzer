---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-mcp: patch
graphql-analyzer-core: patch
graphql-analyzer-eslint-plugin: patch
---

Graduate all packages from the `-alpha` prerelease line to stable. The previous `0.X.Y-alpha.0` GitHub releases captured the actual feature/fix content (browser playground, ESLint plugin parity, etc.); this release just drops the prerelease suffix so the next published versions are normal SemVer ([#1027](https://github.com/trevor-scheer/graphql-analyzer/pull/1027)).
