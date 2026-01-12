---
name: debug-lsp
description: Debug LSP server issues including hangs, incorrect responses, performance problems, or crashes. Use when troubleshooting the language server.
user-invocable: true
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

| Level | Use For |
|-------|---------|
| ERROR | Critical failures only |
| WARN | Non-fatal issues |
| INFO | High-level operations |
| DEBUG | Detailed operations, timing |
| TRACE | Deep debugging |

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

### Hangs / Deadlocks

**Symptoms**: LSP stops responding, CPU stays high

**Likely cause**: Salsa deadlock from concurrent access

**Debug steps**:
1. Enable RUST_LOG=debug
2. Look for "acquiring lock" messages without corresponding releases
3. Check for snapshot not being dropped before setter calls
4. Consult `salsa.md` agent for deadlock patterns

**Common fix**: Ensure snapshots are dropped before mutations:

```rust
// WRONG
let snapshot = db.clone();
let result = snapshot.some_query();
db.set_input(...); // Deadlock!

// RIGHT
let result = {
    let snapshot = db.clone();
    snapshot.some_query()
}; // snapshot dropped
db.set_input(...); // Safe
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
