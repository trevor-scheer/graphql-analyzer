---
graphql-analyzer-eslint-plugin: patch
---

`no-hashtag-description` diagnostics now report a single-position `loc` (start-only) when surfaced through `@graphql-analyzer/eslint-plugin`, matching `@graphql-eslint/eslint-plugin` exactly. The underlying analyzer diagnostic still carries the full comment range — that richness remains visible to the LSP and CLI; only the ESLint adapter strips the end position to mirror graphql-eslint's reporting shape.
