---
graphql-analyzer-lsp: patch
---

Add tracing logs at every lock acquire/release point in `ProjectHost` and `AnalysisHost`, plus Salsa snapshot creation/clone/drop, to help diagnose deadlocks during rapid consecutive schema file edits.
