---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
---

Warn when no schema files match configured patterns ([#620](https://github.com/trevor-scheer/graphql-analyzer/pull/620))

Previously, when schema patterns in `.graphqlrc.yaml` didn't match any files on disk, the tool silently degraded with no indication to the user. CLI commands now show a warning instead of the misleading "Schema loaded successfully" message, and the LSP server shows a notification and publishes a diagnostic on the config file.
