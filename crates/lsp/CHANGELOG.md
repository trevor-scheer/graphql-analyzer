# Changelog

All notable changes to the GraphQL LSP will be documented in this file.

## 0.1.4 (2026-03-05)

### Fixes

- Pre-filter files for field usage analysis using schema coordinates index
- Optimize type name goto-definition with pre-computed index ([#702](https://github.com/trevor-scheer/graphql-analyzer/pull/702))
- Pre-filter files for type reference lookups with per-file index

## 0.1.3 (2026-03-02)

### Features

- Add type name completions in type positions ([#679](https://github.com/trevor-scheer/graphql-analyzer/pull/679))
- Add cross-file diagnostic refresh on save ([#672](https://github.com/trevor-scheer/graphql-analyzer/pull/672))
- Add directive completions and directive argument completions ([#675](https://github.com/trevor-scheer/graphql-analyzer/pull/675))
- Add enum value completions in argument positions ([#674](https://github.com/trevor-scheer/graphql-analyzer/pull/674))
- Add field argument completions ([#673](https://github.com/trevor-scheer/graphql-analyzer/pull/673))
- Add input object field completions ([#678](https://github.com/trevor-scheer/graphql-analyzer/pull/678))
- Add top-level keyword completions ([#677](https://github.com/trevor-scheer/graphql-analyzer/pull/677))

### Fixes

- Fix monolithic fragment/operation index cache invalidation in document validation ([#653](https://github.com/trevor-scheer/graphql-analyzer/pull/653))
- Use targeted field usage analysis for hover instead of whole-project analysis ([#645](https://github.com/trevor-scheer/graphql-analyzer/issues/645))
- Use per-file aggregation queries for incremental unused field/fragment detection ([#646](https://github.com/trevor-scheer/graphql-analyzer/issues/646))
- Use HIR source locations for O(1) goto-definition instead of linear schema scanning ([#656](https://github.com/trevor-scheer/graphql-analyzer/pull/656))
- Add pre-filtering to find-references using cached per-file queries ([#659](https://github.com/trevor-scheer/graphql-analyzer/pull/659))
- Add interface implementors index for O(1) completion lookups ([#654](https://github.com/trevor-scheer/graphql-analyzer/pull/654))
- Replace linear lookups with HashMap for O(1) access in symbols and field usage analysis ([#655](https://github.com/trevor-scheer/graphql-analyzer/pull/655))
- Log Salsa query cache hit/miss at debug level for performance diagnostics ([#668](https://github.com/trevor-scheer/graphql-analyzer/pull/668))

## 0.1.2 (2026-02-24)

### Features

- Add configurable client directive support for Apollo and Relay via extensions.client config option ([#626](https://github.com/trevor-scheer/graphql-analyzer/pull/626))

### Fixes

- Add @oneOf directive to schema builtins so it is recognized in all schemas without being explicitly defined ([#621](https://github.com/trevor-scheer/graphql-analyzer/pull/621))
- Fix spurious validation errors for projects with no schema ([#625](https://github.com/trevor-scheer/graphql-analyzer/pull/625))
- Full IDE support for schema type extensions: multi-location goto-def on type names shows both base type and extensions, correct document symbol labels, order-independent extension merging, directive tracking on all schema elements, and scalar type extension support ([#633](https://github.com/trevor-scheer/graphql-analyzer/pull/633))

#### Strict validation mode and pattern diagnostics ([#620](https://github.com/trevor-scheer/graphql-analyzer/pull/620))

**CLI Changes:**

- `validate` now fails by default if no schema files are found (exit code 2)
- Added `--syntax-only` flag to skip schema validation and only check document syntax
- Fails if no document files are found matching configured patterns

**LSP Changes:**

- Added per-pattern error diagnostics for both `schema` and `documents`: each pattern that matches no files shows an error on the specific pattern in the config
- Added summary error diagnostic on `schema`/`documents` key when ALL patterns fail to match
- Diagnostics now underline just the key name (not the colon)

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
