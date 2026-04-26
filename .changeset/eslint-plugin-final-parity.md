---
graphql-analyzer-eslint-plugin: minor
graphql-analyzer-core: minor
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-mcp: patch
---

`@graphql-analyzer/eslint-plugin` is now a true drop-in replacement for `@graphql-eslint/eslint-plugin`. ESLint `rules: { rule: [severity, options] }` payloads now reach the analyzer; embedded GraphQL in JS/TS hosts is extracted by the processor with positions remapped back to the host file; multi-project `.graphqlrc.yaml` configs route per-file via `getProjectForFile`; all five upstream flat presets ship with byte-for-byte content; the 30 GraphQL spec validation rule names are exposed as no-op stubs so existing configs load cleanly. `naming-convention` and `alphabetize` gain schema-side enforcement and the bulk of upstream's options. ([#1025](https://github.com/trevor-scheer/graphql-analyzer/pull/1025))
