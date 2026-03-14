# Changelog

All notable changes to the GraphQL LSP will be documented in this file.

## 0.1.8 (2026-03-14)

### Fixes

- Fix extension crash on activation due to duplicate `checkStatus` command registration. The status bar item now uses a dedicated `jumpToLogs` command to avoid conflicting with the LSP server's `checkStatus` command.

## 0.1.7 (2026-03-14)

### Features

- Add user-facing trace capture for performance debugging ([#761](https://github.com/trevor-scheer/graphql-analyzer/pull/761))

### Fixes

- Support schema types defined only via `extend type` across schema files ([#756](https://github.com/trevor-scheer/graphql-analyzer/pull/756))
- Fix hover showing 0 usages for fields on nested types ([#742](https://github.com/trevor-scheer/graphql-analyzer/pull/742))
- Fix SWC parse error on `.ts` files containing generic arrow functions ([#765](https://github.com/trevor-scheer/graphql-analyzer/pull/765))
- Add debug logging for schema merge error details ([#737](https://github.com/trevor-scheer/graphql-analyzer/pull/737))
- Only count and load files that contain GraphQL content during project initialization, reducing noise in the file count for projects with many TS/JS files. Remove the "maybe slow" warning popup for large file counts. Clicking the status bar item now opens the debug output channel. ([#759](https://github.com/trevor-scheer/graphql-analyzer/pull/759))
- Include file path in SWC parser error messages instead of "input" ([#736](https://github.com/trevor-scheer/graphql-analyzer/pull/736))

## 0.1.6 (2026-03-09)

### Features

- Add VSCode settings for OpenTelemetry tracing and reduce default log level to warn for better performance on large codebases. OTEL dependencies are now always included (no longer behind a cargo feature flag). ([#724](https://github.com/trevor-scheer/graphql-analyzer/pull/724))

### Fixes

- Add --version / -V flag to graphql-lsp binary ([#727](https://github.com/trevor-scheer/graphql-analyzer/pull/727))

## 0.1.5 (2026-03-06)

### Features

- Support request cancellation via spawn_blocking ([#712](https://github.com/trevor-scheer/graphql-analyzer/pull/712))
- Implement incremental text synchronization for improved editing performance ([#275](https://github.com/trevor-scheer/graphql-analyzer/pull/275))
- Support rename symbol for fragments, operations, and variables ([#717](https://github.com/trevor-scheer/graphql-analyzer/pull/717))
- Add schema keyword completions for type definition documents ([#696](https://github.com/trevor-scheer/graphql-analyzer/pull/696))
- Add signature help for field and directive arguments ([#716](https://github.com/trevor-scheer/graphql-analyzer/pull/716))

### Fixes

- Drop duplicate parse errors that appeared at incorrect positions ([#711](https://github.com/trevor-scheer/graphql-analyzer/pull/711))
- Fix false positive in redundant fields rule for fields with different sub-selections ([#719](https://github.com/trevor-scheer/graphql-analyzer/pull/719))
- Fix UTF-16 position handling for files with non-ASCII characters ([#710](https://github.com/trevor-scheer/graphql-analyzer/pull/710))

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
