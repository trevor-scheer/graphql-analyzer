---
graphql-analyzer-vscode: patch
graphql-analyzer-lsp: patch
---

Fix extension crash on activation due to duplicate `checkStatus` command registration. The status bar item now uses a dedicated `jumpToLogs` command to avoid conflicting with the LSP server's `checkStatus` command.
