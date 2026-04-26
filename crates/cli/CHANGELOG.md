# Changelog

All notable changes to the GraphQL CLI will be documented in this file.

## 0.2.3-alpha.0 (2026-04-26)

### Features

#### `@graphql-analyzer/eslint-plugin`: every shared lint rule is now verified end-to-end against `@graphql-eslint/eslint-plugin` with identical diagnostic counts, messages, and source positions. Behavior changes that align ours to graphql-eslint:

- **Message format** (backticks → double quotes around identifiers): `require-import-fragment`, `require-nullable-fields-with-oneof`, `strict-id-in-types`, `selection-set-depth`, `no-deprecated`, `require-deprecation-date`, and several rules touched by the alphabetize/option-schema work.
- **Diagnostic position**: `no-scalar-result-type-on-mutation`, `relay-connection-types`, `require-deprecation-reason`, and `require-deprecation-date` now point at the relevant type/directive name node (matching graphql-eslint) rather than the field name. `unique-enum-value-names` points at each duplicate value's name token. `require-selections` points at the SelectionSet `{`.
- **Firing condition**: `naming-convention` no longer applies hardcoded `OperationDefinition: PascalCase`/`FragmentDefinition: PascalCase`/`Variable: camelCase` defaults — the rule now no-ops without explicit kind config, matching graphql-eslint.
- **Option schemas**: `alphabetize`, `no-root-type`, `match-document-filename`, `selection-set-depth`, and `require-description` now accept the same option shapes graphql-eslint does (`maxDepth` instead of `max_depth`, kind-filter objects, etc.).
- **Semantics**: `require-deprecation-date` now reads the `@deprecated(deletionDate: "DD/MM/YYYY")` argument (rather than scanning the `reason` substring) and emits the same `MESSAGE_INVALID_FORMAT` / `MESSAGE_INVALID_DATE` / `MESSAGE_CAN_BE_REMOVED` diagnostics graphql-eslint does.
- **Multi-config support**: the napi host now resets per `init()` call, so monorepos with multiple `.graphqlrc.yaml` projects no longer leak schema/document state from one project into another.

#### **Drop-in name parity** with `@graphql-eslint/eslint-plugin`: the three remaining mismatched rule names were renamed so all 31 shared rules now line up 1:1.

- `unused-fields` → `no-unused-fields` (config key: `unusedFields` → `noUnusedFields`)
- `unused-fragments` → `no-unused-fragments` (config key: `unusedFragments` → `noUnusedFragments`)
- `unused-variables` → `no-unused-variables` (config key: `unusedVariables` → `noUnusedVariables`)

This is a breaking change for users who configured these rules under their old names; update `.graphqlrc.yaml` lint config keys accordingly. Migration guide added at `linting/migrating-from-graphql-eslint`.

The ESLint shim now propagates `messageId` and `fix` from the analyzer through to `LintMessage`. The parity test compares `(line, column, endLine, endColumn, message, messageId, fix)` together per diagnostic so any drift across rules surfaces as a clean diff. graphql-eslint emits stable `messageId` values for ~22 shared rules; those are now matched verbatim — both kebab-case ids that mirror the rule name (e.g. `"no-anonymous-operations"`, `"alphabetize"`) and the SHOUTY_SNAKE constants graphql-eslint uses for richer per-site distinctions (e.g. `"HASHTAG_COMMENT"`, `"MISSING_ARGUMENTS"`, `"MESSAGE_REQUIRE_DATE"`, `"MUST_HAVE_CONNECTION_SUFFIX"`).

Behavioral parity tightened on the three newly-aligned rules:

- **`no-unused-fields`** message now reads `Field "X" is unused` (matching graphql-eslint), with the diagnostic still anchored at the field name token.
- **`no-unused-fragments`** message reads `Fragment "X" is never used.` and the diagnostic anchors on the `fragment` keyword token (graphql-js's NoUnusedFragmentsRule range, post graphql-eslint adapter).
- **`no-unused-variables`** message reads `Variable "$name" is never used in operation "Op".` (or `… is never used.` for anonymous ops) and anchors on the `$` sigil — matching graphql-js verbatim.

The `alphabetize` rule now emits a `LintMessage.fix` matching graphql-eslint's swap edit. Other rules that ship internal autofixes (`require-selections`, `no-unused-fragments`, `no-unused-variables`) continue to expose those fixes to LSP/CLI consumers but suppress them in the ESLint shim, since graphql-eslint either ships them as `suggest` or doesn't autofix them at all.

### Fixes

- Extend `description-style` and `require-description` to cover nested AST nodes (fields, arguments, input values, enum values, directives) and — for `require-description` — operation definitions, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1011](https://github.com/trevor-scheer/graphql-analyzer/pull/1011)).
- `require-nullable-result-in-root` now matches `@graphql-eslint/eslint-plugin` exactly: non-null list types like `[User!]!` are no longer flagged (only non-null *named* returns are), and the diagnostic message is `Unexpected non-null result <type> in type "<root>"` to match graphql-eslint's wording.
- `require-selections`: append `` or add to used fragment(s) `X` `` suffix when the missing field is reachable through fragments that don't contain it (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004))
- Type-position diagnostics for `require-field-of-type-query-in-mutation-result` and `require-nullable-result-in-root` now report at the field's return type name node, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1008](https://github.com/trevor-scheer/graphql-analyzer/pull/1008)).

## 0.2.2 (2026-04-19)

### Features

- Add help text, related locations, and documentation URLs to diagnostics ([#934](https://github.com/trevor-scheer/graphql-analyzer/pull/934))
- Add `matchDocumentFilename` lint rule that enforces operation and fragment names match their filename
- Add `relayArguments` lint rule to enforce Relay-compliant pagination arguments on connection fields ([#988](https://github.com/trevor-scheer/graphql-analyzer/pull/988))
- Add `relayConnectionTypes` lint rule ([#984](https://github.com/trevor-scheer/graphql-analyzer/pull/984))
- Add `relayEdgeTypes` lint rule to enforce Relay-compliant edge type definitions ([#992](https://github.com/trevor-scheer/graphql-analyzer/pull/992))
- Add `relayPageInfo` lint rule to enforce Relay PageInfo type specification ([#986](https://github.com/trevor-scheer/graphql-analyzer/pull/986))
- Add `requireDeprecationDate` lint rule
- Add `requireImportFragment` lint rule ([#991](https://github.com/trevor-scheer/graphql-analyzer/pull/991))
- Add `requireTypePatternWithOneof` lint rule enforcing that types with `@oneOf` contain both `ok` and `error` fields

### Fixes

- Add `noRootType` lint rule to disallow certain root type definitions in the schema
- Add `requireNullableFieldsWithOneof` lint rule ([#985](https://github.com/trevor-scheer/graphql-analyzer/pull/985))
- Add `requireNullableResultInRoot` lint rule ([#994](https://github.com/trevor-scheer/graphql-analyzer/pull/994))
- Validate that `resolvedSchema` paths point to existing files ([#982](https://github.com/trevor-scheer/graphql-analyzer/pull/982))

## 0.2.1 (2026-04-16)

### Features

- Add `list-rules` and `explain` CLI commands for rule discovery ([#933](https://github.com/trevor-scheer/graphql-analyzer/pull/933))
- Add SARIF output format for GitHub code scanning ([#943](https://github.com/trevor-scheer/graphql-analyzer/pull/943))

### Fixes

- Improve error messages for config and introspection failures ([#940](https://github.com/trevor-scheer/graphql-analyzer/pull/940))

## 0.2.0 (2026-04-12)

### Breaking Changes

#### Namespace extensions under `extensions.graphql-analyzer` and add resolved schema support ([#966](https://github.com/trevor-scheer/graphql-analyzer/pull/966))

BREAKING: `client`, `lint`, and `extractConfig` must now be nested under `extensions.graphql-analyzer` in `.graphqlrc.yaml`.

New: `resolvedSchema` config option to validate queries against a build-generated schema while keeping source files for navigation.

## 0.1.10 (2026-04-04)

### Features

- Show source code snippets in CLI diagnostic output ([#941](https://github.com/trevor-scheer/graphql-analyzer/pull/941))
- Add `--max-warnings` flag for gradual lint adoption ([#938](https://github.com/trevor-scheer/graphql-analyzer/pull/938))
- Add `requireSelections` lint rule for cache normalization ([#944](https://github.com/trevor-scheer/graphql-analyzer/pull/944))

### Fixes

- Add usage examples and aliases to CLI help text ([#927](https://github.com/trevor-scheer/graphql-analyzer/pull/927))
- Add "did you mean?" suggestions for config typos ([#932](https://github.com/trevor-scheer/graphql-analyzer/pull/932))
- Fix config validation test compilation after glob caching refactor ([#948](https://github.com/trevor-scheer/graphql-analyzer/pull/948))

## 0.1.9 (2026-03-30)

### Fixes

- Fix unused fragment auto-fix in TS/JS files to delete the entire variable declaration instead of just the GraphQL content ([#487](https://github.com/trevor-scheer/graphql-analyzer/issues/487))

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
