---
graphql-analyzer-lsp: patch
---

Fix thread-safety violation where Analysis snapshots shared FileRegistry with AnalysisHost, breaking snapshot isolation ([#714](https://github.com/trevor-scheer/graphql-analyzer/pull/714))
