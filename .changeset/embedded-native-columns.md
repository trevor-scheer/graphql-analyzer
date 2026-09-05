---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-mcp: patch
---

Report embedded GraphQL lint diagnostics and fixes at their physical source columns, including Unicode prefixes.

Suppress native fixes when an extracted block is not an unchanged source substring, because applying its offsets without an escape-aware source map could corrupt host code.
