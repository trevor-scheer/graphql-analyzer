# Changelog

All notable changes to the GraphQL VSCode extension will be documented in this file.

## 0.1.10 (2026-03-14)

### Features

- Add user-facing trace capture for performance debugging ([#761](https://github.com/trevor-scheer/graphql-analyzer/pull/761))

### Fixes

- Support schema types defined only via `extend type` across schema files ([#756](https://github.com/trevor-scheer/graphql-analyzer/pull/756))
- Fix TextMate grammar bugs, dead code, and missing features ([#743](https://github.com/trevor-scheer/graphql-analyzer/pull/743))
- Only count and load files that contain GraphQL content during project initialization, reducing noise in the file count for projects with many TS/JS files. Remove the "maybe slow" warning popup for large file counts. Clicking the status bar item now opens the debug output channel.

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
