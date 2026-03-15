---
description: LSP handler conventions and protocol compliance
paths:
  - "crates/graphql-lsp/**"
---

# LSP Handler Conventions

- All request handlers go in `crates/graphql-lsp/src/handlers/`
- Register new capabilities in the server initialization
- Every handler must support cancellation via the LSP cancellation token
- Return `None`/empty results rather than errors for graceful degradation
- Prefer incremental document sync over full sync
- Never block the async runtime with expensive computation (see below)

## Async Runtime Starvation — Critical Rule

**All Salsa query execution MUST run inside `spawn_blocking`, never directly in an async handler.**

tower-lsp runs handlers on the tokio async runtime. Salsa queries are synchronous and can take
hundreds of milliseconds on large schemas. Running them directly in an async handler starves the
runtime — health checks time out, the client marks the server as dead, and the LSP appears to hang.

### Two helpers exist — use the right one:

| Helper          | Use when                                            | Does what                                                               |
| --------------- | --------------------------------------------------- | ----------------------------------------------------------------------- |
| `with_analysis` | Request handlers (need workspace lookup + snapshot) | Finds project host, acquires snapshot, runs closure in `spawn_blocking` |
| `blocking`      | Notification handlers that already have a snapshot  | Thin `spawn_blocking` wrapper with panic handling                       |

### What counts as "expensive"?

Any Salsa query call: `diagnostics()`, `all_diagnostics_for_file()`, `all_diagnostics_for_change()`,
`all_diagnostics_for_files()`, `field_coverage()`, `goto_definition()`, `find_references()`, etc.
If it touches `Analysis` or `snapshot`, it runs Salsa queries.

### Pattern for notification handlers:

```rust
// WRONG — blocks the async runtime
let diagnostics = snapshot.diagnostics(&changed_file);

// CORRECT — runs on the blocking thread pool
let Some(diagnostics) = Self::blocking(move || {
    snapshot.diagnostics(&changed_file)
}).await else {
    return;
};
```

### Why not just use rust-analyzer's approach?

rust-analyzer avoids this entirely by using a synchronous event loop (crossbeam channels) with an
explicit thread pool — no async runtime to starve. We use tower-lsp which is async, so we must
use `spawn_blocking` to bridge the gap. This is a fundamental architectural constraint.
