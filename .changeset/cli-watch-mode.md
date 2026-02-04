---
graphql-analyzer-cli: minor
---

Add `--watch` flag to validate, lint, and check commands for continuous validation during development

- `graphql validate --watch`: Watch mode for GraphQL spec validation
- `graphql lint --watch`: Watch mode for custom lint rules
- `graphql check --watch`: Watch mode for combined validation + lint (recommended)

Features include:
- Cross-platform file watching using notify crate
- 100ms debouncing for rapid file changes
- Human-readable output with timestamps and colored status
- JSON streaming output for tooling integration (`--format json`)
- Incremental revalidation via Salsa cache
