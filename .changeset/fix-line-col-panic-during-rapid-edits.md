---
graphql-analyzer-lsp: patch
---

Fix LSP server panics during rapid schema edits. `LineIndex::line_col` previously asserted that the byte offset was in-bounds and on a char boundary, panicking the `spawn_blocking` worker when a Salsa-cached lint diagnostic span survived a content edit and was then converted against a freshly-built `LineIndex` for the new (shorter) source. The function now clamps stale offsets to the end of source and snaps mid-character offsets to the nearest preceding boundary, emitting a `tracing::warn!` so the upstream bug stays visible without crashing the server. Also makes the `Uri::from_str` call sites in the `code_action` and `code_lens` handlers fall back to skipping the request rather than panicking, and installs a global panic hook plus a `JoinError`-payload extractor so future panics surface their actual message and backtrace in the logs instead of the useless `task N panicked`.
