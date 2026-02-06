---
graphql-analyzer-lsp: minor
---

Add comprehensive autocompletion support for GraphQL documents ([#478](https://github.com/trevor-scheer/graphql-analyzer/pull/478))

Implements context-aware completions for:
- Field completions in selection sets (with required argument snippets)
- Argument completions for fields and directives
- Fragment spread completions
- Variable completions (references to operation-defined variables)
- Directive completions (`@skip`, `@include`) with location awareness
- Type completions in variable definitions (input types only)
- Enum value completions
- Inline fragment type completions for unions and interfaces
