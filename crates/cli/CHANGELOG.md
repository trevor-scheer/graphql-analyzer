# Changelog

All notable changes to the GraphQL CLI will be documented in this file.

## 0.1.2 (2026-02-21)

### Fixes

#### Fix validate command not reporting errors from document files ([#617](https://github.com/trevor-scheer/graphql-analyzer/pull/617))

The CLI and MCP validate commands were silently ignoring validation errors from document files (TypeScript, JavaScript, GraphQL) due to a path format mismatch. Files were registered with raw filesystem paths but looked up with file:// URIs, causing lookups to fail.

## 0.1.1 (2026-02-12)

### Features

#### Add `--watch` flag to validate, lint, and check commands for continuous validation during development ([#467](https://github.com/trevor-scheer/graphql-analyzer/pull/467))

- `graphql validate --watch`: Watch mode for GraphQL spec validation
- `graphql lint --watch`: Watch mode for custom lint rules
- `graphql check --watch`: Watch mode for combined validation + lint (recommended)

Features include:
- Cross-platform file watching using notify crate
- 100ms debouncing for rapid file changes
- Human-readable output with timestamps and colored status
- JSON streaming output for tooling integration (`--format json`)
- Incremental revalidation via Salsa cache

#### Support schema definitions in TypeScript/JavaScript files ([#561](https://github.com/trevor-scheer/graphql-analyzer/pull/561))

Schema files configured via `.graphqlrc.yaml` can now be TypeScript or JavaScript files containing GraphQL schema definitions in tagged template literals (e.g. `gql\`type User { ... }\``). Diagnostics, linting, and validation all report correct line/column positions within the original TS/JS file.

### Fixes

- Fix cargo audit vulnerabilities by updating dependencies (bytes, time, git2, vergen-git2, indicatif, rmcp) ([#563](https://github.com/trevor-scheer/graphql-analyzer/pull/563))
- Fix false "fragment defined multiple times" errors in TypeScript/JavaScript files with multiple gql blocks ([#594](https://github.com/trevor-scheer/graphql-analyzer/pull/594))

## 0.1.0 (2026-02-02)

### Features

- Initial release

## 0.1.0-alpha.13 (2026-02-02)

### Fixes

- Add standalone graphql-lsp and graphql-mcp binaries

## 0.1.0-alpha.12 (2026-02-02)

### Fixes

- Fix ARM64 Linux cross-compilation by switching from native-tls to rustls

## 0.1.0-alpha.11 (2026-02-01)

### Fixes

- Fix ARM64 Linux cross-compilation by installing OpenSSL in Docker container

## 0.1.0-alpha.10 (2026-02-01)

### Fixes

- Fix release workflow: add ARM64 Linux builds using cross, fix changeset consumption

## 0.1.0-alpha.9 (2026-02-01)

### Fixes

- Initial release with multi-package versioning

## 0.1.0-alpha.8 (2026-02-01)

### Fixes

- Initial release with multi-package versioning

## 0.1.0-alpha.7 (2026-02-01)

### Fixes

- Initial release with multi-package versioning
