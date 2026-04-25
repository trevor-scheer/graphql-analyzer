---
name: debug-lsp
description: Debug LSP server issues including hangs, incorrect responses, performance problems, or crashes. Use when troubleshooting the language server.
user-invocable: true
argument-hint: "[symptom description]"
allowed-tools: Bash(cargo build *), Bash(cargo test *), Bash(cargo run *), Bash(RUST_LOG=*), Read, Grep, Glob
---

# Debugging the LSP Server

Follow this guide when debugging LSP issues.

## Quick Diagnostics

### 1. Check if LSP Binary Exists

```bash
ls -la target/debug/graphql-lsp
```

If missing, rebuild:

```bash
cargo build
```

### 2. Test LSP Directly

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | target/debug/graphql-lsp
```

Should return a valid JSON-RPC response.

## Logging

### Enable Debug Logging

```bash
RUST_LOG=debug target/debug/graphql-lsp
```

### Module-Specific Logging

```bash
# LSP layer only
RUST_LOG=graphql_lsp=debug target/debug/graphql-lsp

# Analysis layer
RUST_LOG=graphql_analysis=debug target/debug/graphql-lsp

# Multiple modules
RUST_LOG=graphql_lsp=debug,graphql_analysis=info,graphql_hir=trace target/debug/graphql-lsp
```

### Log Levels

| Level | Use For                     |
| ----- | --------------------------- |
| ERROR | Critical failures only      |
| WARN  | Non-fatal issues            |
| INFO  | High-level operations       |
| DEBUG | Detailed operations, timing |
| TRACE | Deep debugging              |

## OpenTelemetry Tracing

For performance issues, use distributed tracing:

### 1. Build with OpenTelemetry

```bash
cargo build --features otel
```

### 2. Start Jaeger

```bash
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

### 3. Run with Tracing

```bash
OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp
```

### 4. View Traces

Open http://localhost:16686 in your browser.

Look for:

- Long spans indicating slow operations
- Missing spans indicating crashes
- Repeated spans indicating unnecessary recomputation

## Common Issues

### LSP Not Responding

**Symptoms**: Editor shows no diagnostics, features don't work

**Debug steps**:

1. Check VSCode Output → GraphQL for errors
2. Verify LSP binary path in extension settings
3. Test LSP directly (see above)
4. Check for panics in logs

### Panics in worker-pool tasks

**Symptoms**: `tracing::error!` from the global panic hook, then an `InternalError` JSON-RPC response (id intact). Individual requests fail but the server stays up.

**What to look for**:

- The threadpool worker wraps each task in `std::panic::catch_unwind(AssertUnwindSafe(...))` (see `GlobalState::spawn_with_snapshot`). On a caught panic the worker sends back a `Response::new_err(id, ErrorCode::InternalError, "internal error: handler panicked")`, so the request id always gets a response and `in_flight` is cleared.
- The global panic hook (installed in `install_panic_hook` in `crates/lsp/src/lib.rs`) emits `tracing::error!` with the message, source location, and a backtrace if `RUST_BACKTRACE=1` is set. Set the env var on the LSP server process to get backtraces.
- Common offenders: stale byte offsets in cached lint diagnostics colliding with a freshly-built `LineIndex` after rapid edits. `LineIndex::line_col` clamps and warns rather than panicking, but if you see `LineIndex::line_col offset is past end of source` or `landed mid-character` warnings, that's a pre-existing bug somewhere in the diagnostics pipeline that needs investigation.

### Hangs / Deadlocks

**Symptoms**: LSP stops responding, CPU stays low.

**Background**: After the sync-LSP migration, `GlobalState`/`AnalysisHost` mutations only happen on the main thread, and snapshots live on workers. The whole pre-migration deadlock class (snapshot blocked on a side-channel `RwLock` while the main thread held the writer) is gone. A hang today is much more likely to be:

- The main thread is stuck inside a `select!` arm — e.g. a notification handler is running a long Salsa query inline instead of using `spawn_diagnostics_for_uri` / `spawn_with_snapshot`. Workers idle while the main thread blocks; the `recv(connection.receiver)` arm never fires.
- A worker is parked inside a Salsa setter. This shouldn't happen, because setters only run on the main thread — if you see it, somebody added a setter to a `threadpool::execute` closure. Don't.
- The introspection thread blocked on a request whose result the main thread isn't draining (look for `recv(state.introspection_result_receiver)` not firing).

**Debug steps**:

1. Enable `RUST_LOG=debug`. Look at which `select!` arm last fired in the main loop.
2. If a notification handler is taking a long time, check whether it's running Salsa queries inline. Move them onto the pool with `spawn_diagnostics_for_uri` / `spawn_diagnostics_batch`.
3. If a request never gets a response, check `state.in_flight` — the request is either still in flight on a worker, or its response was dropped because the id was no longer in `in_flight` (cancelled).
4. If a snapshot is blocking a setter, find the worker that's still holding it and confirm it isn't itself trying to call a setter. The "use a snapshot, drop it, then mutate" lifecycle rule still applies on the main thread.

**The structural rule** (see `crates/CLAUDE.md` "Snapshot Safety"): `Analysis` snapshots and `AnalysisHost` must not share any non-Salsa lock. If you need snapshot-visible data, put it in a Salsa input. If you find yourself adding an `Arc<RwLock<...>>` to `AnalysisHost` whose contents a snapshot would also read, **stop and reach for a `#[salsa::input]` instead**.

**The lifecycle rule** still applies on the main thread when a notification handler holds a snapshot and then writes:

```rust
// WRONG
let snapshot = host.snapshot();
let result = snapshot.some_query();
host.add_file(...); // Hangs: snapshot still alive on this thread

// RIGHT — let update_file_and_snapshot do the write+snapshot atomically:
let (_is_new, snapshot) = host.update_file_and_snapshot(&file_path, &content, lang, kind);
// snapshot is now safe to send to a worker
```

### Incorrect Diagnostics

**Symptoms**: Wrong errors, missing errors, stale errors

**Debug steps**:

1. Check file is registered in project
2. Verify schema is loaded correctly
3. Check fragment resolution with `all_fragments()` query
4. Look for cache invalidation issues

### Slow Performance

**Symptoms**: Laggy editor, delayed diagnostics

**Debug steps**:

1. Use OpenTelemetry to identify hot spots
2. Check if warm queries are being recomputed (should be cached)
3. Look for O(n) operations that should be O(1)
4. Run benchmarks: `cargo bench`

**Expected performance**:

- Warm queries: < 1ms
- Cold parse: < 10ms for typical files
- Full validation: < 100ms for typical project

### VSCode Extension Issues

**Symptoms**: Extension not activating, wrong files targeted

**Debug steps**:

1. Check extension logs: View → Output → GraphQL
2. Verify `documentSelector` includes target languages
3. Check grammar injection for syntax highlighting
4. Rebuild extension: `cd editors/vscode && npm run compile`

## SME Agents to Consult

- **salsa.md**: For deadlocks, cache issues, incremental computation bugs
- **lsp.md**: For protocol violations, response format issues
- **rust-analyzer.md**: For architectural debugging patterns
- **vscode-extension.md**: For extension-specific issues
