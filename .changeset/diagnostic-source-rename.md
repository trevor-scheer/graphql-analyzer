---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
---

Rename diagnostic `source` values to short, semantic labels: parser/`apollo-compiler` errors are now reported as `syntax`/`validation`, and project-wide unused-field/-fragment warnings are reattributed from `graphql-analysis` to `graphql-linter` ([#TBD](https://github.com/trevor-scheer/graphql-analyzer/pull/TBD))
