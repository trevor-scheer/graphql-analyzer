# Changelog

All notable changes to the GraphQL CLI will be documented in this file.

## 0.1.8 (2026-03-29)

### Features

- Rename lint rule names from snake_case to camelCase for consistency with config format ([#811](https://github.com/trevor-scheer/graphql-analyzer/pull/811))
- Add environment variable interpolation (`${VAR}` and `${VAR:default}`) in config files ([#788](https://github.com/trevor-scheer/graphql-analyzer/pull/788))
- Support JSON introspection result files as schema source ([#789](https://github.com/trevor-scheer/graphql-analyzer/pull/789))
- Support inline lint ignore comments for per-case suppression of lint rules
- Support package.json "graphql" key for config discovery ([#791](https://github.com/trevor-scheer/graphql-analyzer/pull/791))
- Add noDuplicateFields, noUnreachableTypes, requireDeprecationReason, noHashtagDescription, and uniqueEnumValueNames to the recommended lint preset
- Add TOML config format support (.graphqlrc.toml, graphql.config.toml) ([#792](https://github.com/trevor-scheer/graphql-analyzer/pull/792))
- Support URL-with-headers inline schema syntax from graphql-config standard ([#790](https://github.com/trevor-scheer/graphql-analyzer/pull/790))
- Add Vue, Svelte, and Astro framework support for GraphQL extraction ([#787](https://github.com/trevor-scheer/graphql-analyzer/pull/787))

### Fixes

- Add `alphabetize` lint rule to enforce alphabetical ordering of fields, arguments, and variables ([#614](https://github.com/trevor-scheer/graphql-analyzer/pull/614))
- Add `descriptionStyle` lint rule: Enforces consistent description style (block vs inline) (broken out from #613)
- Disable ANSI escape codes in tracing output ([#794](https://github.com/trevor-scheer/graphql-analyzer/pull/794))
- Add `inputName` lint rule: Enforces that input type names end with a specific suffix (broken out from #613)
- Add `loneExecutableDefinition` lint rule: Requires each file to contain only one executable definition (operation or fragment) (broken out from #613)
- Add `namingConvention` lint rule: Enforces naming conventions for operations, fragments, and variables (broken out from #613)
- Add `noDuplicateFields` lint rule: Disallows duplicate fields within the same selection set (broken out from #613)
- Add `noHashtagDescription` lint rule: Disallows using # comments as type descriptions in schema (broken out from #613)
- Add `noOnePlaceFragments` lint rule: Detects fragments that are used in only one place and could be inlined (broken out from #613)
- Add `noScalarResultTypeOnMutation` lint rule: Disallows scalar return types on mutation fields (broken out from #613)
- Add `noTypenamePrefix` lint rule: Disallows field names that are prefixed with their parent type name (broken out from #613)
- Add `noUnreachableTypes` lint rule: Detects types that are not reachable from any root operation type (broken out from #613)
- Add `requireFieldOfTypeQueryInMutationResult` lint rule: Requires mutation result types to include a field of the Query type (broken out from #613)
- Add `requireDeprecationReason` and `requireDescription` lint rules (broken out from #613)
- Add schema lint rule execution infrastructure ([#812](https://github.com/trevor-scheer/graphql-analyzer/pull/812))
- Add `selectionSetDepth` lint rule: Limits the depth of selection set nesting to prevent overly complex queries (broken out from #613)
- Add `strictIdInTypes` lint rule: Requires object types to have an ID field (broken out from #613)
- Add `uniqueEnumValueNames` lint rule: Detects duplicate enum value names across different enum types (broken out from #613)

## 0.1.7 (2026-03-14)

### Fixes

- Support schema types defined only via `extend type` across schema files ([#756](https://github.com/trevor-scheer/graphql-analyzer/pull/756))
- Fix hover showing 0 usages for fields on nested types ([#742](https://github.com/trevor-scheer/graphql-analyzer/pull/742))
- Fix SWC parse error on `.ts` files containing generic arrow functions ([#765](https://github.com/trevor-scheer/graphql-analyzer/pull/765))
- Add debug logging for schema merge error details ([#737](https://github.com/trevor-scheer/graphql-analyzer/pull/737))
- Include file path in SWC parser error messages instead of "input" ([#736](https://github.com/trevor-scheer/graphql-analyzer/pull/736))

## 0.1.6 (2026-03-09)

### Fixes

- Add VSCode settings for OpenTelemetry tracing and reduce default log level to warn for better performance on large codebases. OTEL dependencies are now always included (no longer behind a cargo feature flag). ([#724](https://github.com/trevor-scheer/graphql-analyzer/pull/724))

## 0.1.5 (2026-03-06)

### Fixes

- Fix false positive in redundant fields rule for fields with different sub-selections ([#719](https://github.com/trevor-scheer/graphql-analyzer/pull/719))
- Fix UTF-16 position handling for files with non-ASCII characters ([#710](https://github.com/trevor-scheer/graphql-analyzer/pull/710))

## 0.1.4 (2026-03-02)

### Fixes

- Log Salsa query cache hit/miss at debug level for performance diagnostics ([#668](https://github.com/trevor-scheer/graphql-analyzer/pull/668))

## 0.1.3 (2026-02-24)

### Features

- Add configurable client directive support for Apollo and Relay via extensions.client config option ([#626](https://github.com/trevor-scheer/graphql-analyzer/pull/626))

#### Strict validation mode and pattern diagnostics ([#620](https://github.com/trevor-scheer/graphql-analyzer/pull/620))

**CLI Changes:**

- `validate` now fails by default if no schema files are found (exit code 2)
- Added `--syntax-only` flag to skip schema validation and only check document syntax
- Fails if no document files are found matching configured patterns

**LSP Changes:**

- Added per-pattern error diagnostics for both `schema` and `documents`: each pattern that matches no files shows an error on the specific pattern in the config
- Added summary error diagnostic on `schema`/`documents` key when ALL patterns fail to match
- Diagnostics now underline just the key name (not the colon)

### Fixes

- Add @oneOf directive to schema builtins so it is recognized in all schemas without being explicitly defined ([#621](https://github.com/trevor-scheer/graphql-analyzer/pull/621))

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
