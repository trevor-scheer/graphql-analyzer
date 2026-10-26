---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-mcp: patch
---

Report embedded GraphQL lint diagnostics and fixes at their physical source columns, including Unicode prefixes ([#1134](https://github.com/trevor-scheer/graphql-analyzer/pull/1134)).

Suppress native fixes when an extracted block is not an unchanged source substring, because applying its offsets without an escape-aware source map could corrupt host code.

Use the file's host language to extract component documents, including Svelte and Astro. Recognize kebab-case rule names and the `@graphql-analyzer/` and `@graphql-eslint/` prefixes in native ESLint suppression directives.
