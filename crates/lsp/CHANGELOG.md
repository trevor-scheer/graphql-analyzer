# Changelog

All notable changes to the GraphQL LSP will be documented in this file.

## 0.1.1 (2026-02-12)

### Features

#### Support schema definitions in TypeScript/JavaScript files ([#561](https://github.com/trevor-scheer/graphql-analyzer/pull/561))

Schema files configured via `.graphqlrc.yaml` can now be TypeScript or JavaScript files containing GraphQL schema definitions in tagged template literals (e.g. `gql\`type User { ... }\``). Diagnostics, linting, and validation all report correct line/column positions within the original TS/JS file.

### Fixes

- Fix cargo audit vulnerabilities by updating dependencies (bytes, time, git2, vergen-git2, indicatif, rmcp) ([#563](https://github.com/trevor-scheer/graphql-analyzer/pull/563))

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
