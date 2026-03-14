---
graphql-analyzer-lsp: patch
graphql-analyzer-vscode: patch
---

Only count and load files that contain GraphQL content during project initialization, reducing noise in the file count for projects with many TS/JS files. Remove the "maybe slow" warning popup for large file counts. Clicking the status bar item now opens the debug output channel. ([#759](https://github.com/trevor-scheer/graphql-analyzer/pull/759))
