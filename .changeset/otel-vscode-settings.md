---
graphql-analyzer-lsp: minor
graphql-analyzer-cli: patch
graphql-analyzer-vscode: minor
---

Add VSCode settings for OpenTelemetry tracing and reduce default log level to warn for better performance on large codebases. OTEL dependencies are now always included (no longer behind a cargo feature flag). ([#724](https://github.com/trevor-scheer/graphql-analyzer/pull/724))
