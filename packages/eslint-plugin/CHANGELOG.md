# Changelog

All notable changes to `@graphql-analyzer/eslint-plugin` will be documented in
this file.
## 0.1.1-alpha.0 (2026-04-25)

### Features

#### Initial release of `@graphql-analyzer/eslint-plugin` and the `@graphql-analyzer/core` native addon. ([#1002](https://github.com/trevor-scheer/graphql-analyzer/pull/1002))

- Drop-in replacement for `@graphql-eslint/eslint-plugin` — same plugin names, rule names, and flat-config preset names (`flat/schema-recommended`, `flat/operations-recommended`).
- Native performance via the Rust analyzer through a napi-rs binding.
- Configuration via `.graphqlrc.yaml` under `extensions.graphql-analyzer.lint`, with auto-discovery from the linted file's directory.
- Embedded GraphQL extraction from TypeScript, JavaScript, Vue, Svelte, and Astro.
- ESLint 8.40+ and ESLint 9.x supported (flat config only).

### Fixes

- Extend `description-style` and `require-description` to cover nested AST nodes (fields, arguments, input values, enum values, directives) and — for `require-description` — operation definitions, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1011](https://github.com/trevor-scheer/graphql-analyzer/pull/1011)).
- `no-hashtag-description` diagnostics now report a single-position `loc` (start-only) when surfaced through `@graphql-analyzer/eslint-plugin`, matching `@graphql-eslint/eslint-plugin` exactly. The underlying analyzer diagnostic still carries the full comment range — that richness remains visible to the LSP and CLI; only the ESLint adapter strips the end position to mirror graphql-eslint's reporting shape.
- `require-nullable-result-in-root` now matches `@graphql-eslint/eslint-plugin` exactly: non-null list types like `[User!]!` are no longer flagged (only non-null *named* returns are), and the diagnostic message is `Unexpected non-null result <type> in type "<root>"` to match graphql-eslint's wording.
- Type-position diagnostics for `require-field-of-type-query-in-mutation-result` and `require-nullable-result-in-root` now report at the field's return type name node, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1008](https://github.com/trevor-scheer/graphql-analyzer/pull/1008)).
