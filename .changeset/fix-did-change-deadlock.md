---
graphql-analyzer-lsp: patch
graphql-analyzer-vscode: patch
---

Move diagnostics computation in did_change to blocking thread to prevent async runtime starvation on large schema changes
