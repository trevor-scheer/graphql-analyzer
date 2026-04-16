# Changelog

All notable changes to the GraphQL VSCode extension will be documented in this file.

## 0.2.1 (2026-04-16)

### Fixes

- Publish to Open VSX Registry ([#979](https://github.com/trevor-scheer/graphql-analyzer/pull/979))

## 0.2.0 (2026-04-12)

### Breaking Changes

#### Namespace extensions under `extensions.graphql-analyzer` and add resolved schema support ([#966](https://github.com/trevor-scheer/graphql-analyzer/pull/966))

BREAKING: `client`, `lint`, and `extractConfig` must now be nested under `extensions.graphql-analyzer` in `.graphqlrc.yaml`.

New: `resolvedSchema` config option to validate queries against a build-generated schema while keeping source files for navigation.

### Features

- Add directive IDE features: go-to-definition, hover, find references, and document/workspace symbols ([#969](https://github.com/trevor-scheer/graphql-analyzer/pull/969))
- Show inlay type hints on non-leaf (object type) fields in queries ([#970](https://github.com/trevor-scheer/graphql-analyzer/pull/970))

## 0.1.18 (2026-04-10)

### Fixes

- Bundle the LSP fix for panics during rapid schema edits.

## 0.1.17 (2026-04-07)

### Fixes

- Bundle updated LSP server with lock acquire/release tracing to help diagnose deadlocks during rapid consecutive schema file edits ([#958](https://github.com/trevor-scheer/graphql-analyzer/pull/958))

## 0.1.16 (2026-04-04)

### Features

- Add `require-selections` lint rule for cache normalization ([#944](https://github.com/trevor-scheer/graphql-analyzer/pull/944))

### Fixes

- Add "did you mean?" suggestions for config typos ([#932](https://github.com/trevor-scheer/graphql-analyzer/pull/932))
- Fix LSP deadlock during rapid schema edits in large codebases ([#949](https://github.com/trevor-scheer/graphql-analyzer/pull/949))
- Improve VS Code extension marketplace metadata ([#935](https://github.com/trevor-scheer/graphql-analyzer/pull/935))

## 0.1.15 (2026-03-29)

### Features

- Rename lint rule names from snake_case to camelCase for consistency with config format ([#811](https://github.com/trevor-scheer/graphql-analyzer/pull/811))
- Support inline lint ignore comments for per-case suppression of lint rules
- Add Vue, Svelte, and Astro framework support for GraphQL extraction ([#787](https://github.com/trevor-scheer/graphql-analyzer/pull/787))

### Fixes

- Fix parentheses in `graphql()` calls being colored as GraphQL instead of TypeScript ([#800](https://github.com/trevor-scheer/graphql-analyzer/pull/800))
- Fix report issue command URL encoding ([#793](https://github.com/trevor-scheer/graphql-analyzer/pull/793))
- Add documentation for 17 new lint rules (broken out from #613)
- Reset extension health state and status bar on server restart

## 0.1.14 (2026-03-16)

### Fixes

- Fix deadlock when textDocument/didSave arrives immediately after textDocument/didChange ([#784](https://github.com/trevor-scheer/graphql-analyzer/pull/784))
- Fix missing semantic validation when schema has build errors (duplicate types, etc.) ([#783](https://github.com/trevor-scheer/graphql-analyzer/pull/783))

## 0.1.13 (2026-03-15)

### Fixes

- Move diagnostics computation in did_change to blocking thread to prevent async runtime starvation on large schema changes
- Fix panic on goto definition, find references, and code lens for fields from type extensions across files ([#778](https://github.com/trevor-scheer/graphql-analyzer/pull/778))

## 0.1.12 (2026-03-14)

### Fixes

- Use project extract config during document discovery instead of defaults ([#769](https://github.com/trevor-scheer/graphql-analyzer/pull/769))

## 0.1.11 (2026-03-14)

### Fixes

- Fix extension crash on activation due to duplicate `checkStatus` command registration. The status bar item now uses a dedicated `jumpToLogs` command to avoid conflicting with the LSP server's `checkStatus` command.

## 0.1.10 (2026-03-14)

### Features

- Add user-facing trace capture for performance debugging ([#761](https://github.com/trevor-scheer/graphql-analyzer/pull/761))

### Fixes

- Support schema types defined only via `extend type` across schema files ([#756](https://github.com/trevor-scheer/graphql-analyzer/pull/756))
- Fix hover showing 0 usages for fields on nested types ([#742](https://github.com/trevor-scheer/graphql-analyzer/pull/742))
- Fix TextMate grammar bugs, dead code, and missing features ([#743](https://github.com/trevor-scheer/graphql-analyzer/pull/743))
- Fix SWC parse error on `.ts` files containing generic arrow functions ([#765](https://github.com/trevor-scheer/graphql-analyzer/pull/765))
- Add debug logging for schema merge error details ([#737](https://github.com/trevor-scheer/graphql-analyzer/pull/737))
- Only count and load files that contain GraphQL content during project initialization, reducing noise in the file count for projects with many TS/JS files. Remove the "maybe slow" warning popup for large file counts. Clicking the status bar item now opens the debug output channel. ([#759](https://github.com/trevor-scheer/graphql-analyzer/pull/759))
- Include file path in SWC parser error messages instead of "input" ([#736](https://github.com/trevor-scheer/graphql-analyzer/pull/736))

## 0.1.9 (2026-03-09)

### Features

- Add VSCode settings for OpenTelemetry tracing and reduce default log level to warn for better performance on large codebases. OTEL dependencies are now always included (no longer behind a cargo feature flag). ([#724](https://github.com/trevor-scheer/graphql-analyzer/pull/724))

## 0.1.8 (2026-03-06)

### Features

- Support rename symbol for fragments, operations, and variables ([#717](https://github.com/trevor-scheer/graphql-analyzer/pull/717))
- Add schema keyword completions for type definition documents ([#696](https://github.com/trevor-scheer/graphql-analyzer/pull/696))
- Add signature help for field and directive arguments ([#716](https://github.com/trevor-scheer/graphql-analyzer/pull/716))

### Fixes

- Drop duplicate parse errors that appeared at incorrect positions ([#711](https://github.com/trevor-scheer/graphql-analyzer/pull/711))
- Fix false positive in redundant fields rule for fields with different sub-selections ([#719](https://github.com/trevor-scheer/graphql-analyzer/pull/719))
- Fix UTF-16 position handling for files with non-ASCII characters ([#710](https://github.com/trevor-scheer/graphql-analyzer/pull/710))

## 0.1.7 (2026-03-02)

### Features

- Add cross-file diagnostic refresh on save ([#672](https://github.com/trevor-scheer/graphql-analyzer/pull/672))

## 0.1.6 (2026-02-24)

### Features

- Add configurable client directive support for Apollo and Relay via extensions.client config option ([#626](https://github.com/trevor-scheer/graphql-analyzer/pull/626))
- Add "Report Issue" command that opens GitHub with pre-filled environment diagnostics ([#639](https://github.com/trevor-scheer/graphql-analyzer/pull/639))

### Fixes

- Add @oneOf directive to schema builtins so it is recognized in all schemas without being explicitly defined ([#621](https://github.com/trevor-scheer/graphql-analyzer/pull/621))
- Fix TextMate grammar for body-less type extensions (e.g. `extend type User implements Node`) breaking syntax highlighting on subsequent lines ([#638](https://github.com/trevor-scheer/graphql-analyzer/pull/638))
- Fix VSIX packaging including entire monorepo due to npm workspace dependency resolution ([#638](https://github.com/trevor-scheer/graphql-analyzer/pull/638))
- Contribute GraphQL config schema for automatic JSON/YAML validation in VS Code ([#623](https://github.com/trevor-scheer/graphql-analyzer/pull/623))
- Support relative paths in `graphql-analyzer.server.path` setting, resolved against the workspace folder ([#620](https://github.com/trevor-scheer/graphql-analyzer/pull/620))

## 0.1.5 (2026-02-21)

### Fixes

- Fix language server restart failing on first attempt when server is unresponsive ([#611](https://github.com/trevor-scheer/graphql-analyzer/pull/611))

## 0.1.4 (2026-02-14)

### Fixes

- Fix VS Code Marketplace publisher ID

## 0.1.3 (2026-02-14)

### Fixes

- Include the extension icon in the VS Code Marketplace by adding it to the .vscodeignore file.
- Add extension icon for VS Code Marketplace

## 0.1.2 (2026-02-13)

### Fixes

- Add automated VS Code Marketplace publishing to release workflow ([#596](https://github.com/trevor-scheer/graphql-analyzer/pull/596))

## 0.1.1 (2026-02-12)

### Features

- Add dedicated LSP output channel and rename settings namespace to `graphql-analyzer.*` ([#559](https://github.com/trevor-scheer/graphql-analyzer/pull/559))

### Fixes

- Fix extension failing to load due to missing vscode-languageclient module ([#557](https://github.com/trevor-scheer/graphql-analyzer/pull/557))
- Fix syntax highlighting for gql tags with backtick on separate line ([#529](https://github.com/trevor-scheer/graphql-analyzer/pull/529))

## 0.1.0 (2026-02-02)

### Features

- Initial release

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
