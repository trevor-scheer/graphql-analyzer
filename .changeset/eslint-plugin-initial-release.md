---
graphql-analyzer-core: minor
graphql-analyzer-eslint-plugin: minor
---

Initial release of `@graphql-analyzer/eslint-plugin` and the `@graphql-analyzer/core` native addon. ([#1002](https://github.com/trevor-scheer/graphql-analyzer/pull/1002))

- Drop-in replacement for `@graphql-eslint/eslint-plugin` — same plugin names, rule names, and flat-config preset names (`flat/schema-recommended`, `flat/operations-recommended`).
- Native performance via the Rust analyzer through a napi-rs binding.
- Configuration via `.graphqlrc.yaml` under `extensions.graphql-analyzer.lint`, with auto-discovery from the linted file's directory.
- Embedded GraphQL extraction from TypeScript, JavaScript, Vue, Svelte, and Astro.
- ESLint 8.40+ and ESLint 9.x supported (flat config only).
