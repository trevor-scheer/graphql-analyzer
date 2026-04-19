# Crates - Claude Guide

Architecture, key concepts, and invariants for the Rust crate workspace.

---

## Crate Dependency Graph

```
graphql-lsp / graphql-cli / graphql-mcp    (entrypoints)
    ↓
graphql-ide          (Editor API, POD types)
    ↓
graphql-analysis     (Validation + Linting)
    ↓
graphql-hir          (Semantic layer, structure/body separation)
    ↓
graphql-syntax       (Parsing, TS/JS extraction)
    ↓
graphql-db           (Salsa database, FileId, memoization)
```

### Crate Purposes

| Crate        | Purpose                                    |
| ------------ | ------------------------------------------ |
| `base-db`    | Salsa database foundation, FileId          |
| `syntax`     | GraphQL parser, TS/JS extraction           |
| `hir`        | Semantic layer, structure/body separation  |
| `analysis`   | Validation and diagnostics                 |
| `ide-db`     | IDE-specific database queries              |
| `ide`        | IDE features (hover, completion, goto-def) |
| `linter`     | Lint rules and configuration               |
| `lsp`        | Language Server Protocol implementation    |
| `cli`        | Command-line interface                     |
| `mcp`        | Model Context Protocol server              |
| `config`     | Project configuration (.graphqlrc.yaml)    |
| `introspect` | Remote schema introspection via HTTP       |
| `extract`    | GraphQL extraction from TS/JS files        |
| `apollo-ext` | Apollo-specific extensions                 |
| `types`      | Shared type definitions                    |
| `test-utils` | Testing utilities and fixtures             |

---

> Detailed rules for Salsa queries, GraphQL document model, and cache invariants
> are in `.claude/rules/` (loaded automatically when working in relevant crates).

## Protected Core Features

These features must NOT be removed or degraded:

| Feature                       | Why Critical                      | What Enables It                                        |
| ----------------------------- | --------------------------------- | ------------------------------------------------------ |
| **Embedded GraphQL in TS/JS** | Most users write queries in TS/JS | `documentSelector` includes TS/JS in VS Code extension |
| **Real-time diagnostics**     | Users expect immediate feedback   | LSP `textDocument/didChange` notifications             |
| **Project-wide fragments**    | Fragments span many files         | `all_fragments()` indexes entire project               |

**Solve performance problems without removing features.** Use filtering, lazy evaluation, debouncing, or configuration options instead.

---

## LSP Threading Model

The LSP server uses a sync, single-threaded main loop (not async):

- **Main thread**: Owns all mutable state (`GlobalState`, `WorkspaceManager`, `AnalysisHost` instances). All state mutations happen here. No locks needed.
- **Worker thread pool**: Executes read-only Salsa queries via `Analysis` snapshots. Results return to the main thread via crossbeam channel.
- **Introspection thread**: Dedicated thread with a Tokio runtime for async HTTP introspection calls only.

Snapshots are taken on the main thread and moved to workers. Workers never touch `GlobalState` or `AnalysisHost`. The main thread never blocks on workers — it processes their results as events in the next loop iteration.

This eliminates the entire class of async/sync boundary deadlocks that the previous tower-lsp architecture was vulnerable to. There are no `DashMap`, `tokio::sync::Mutex`, or `Arc<RwLock<...>>` in the LSP crate.

## Snapshot Safety (Still Applies)

**`Analysis` snapshots and `AnalysisHost` MUST NOT share any non-Salsa lock.**

Salsa setters block until all outstanding snapshots are dropped. In the sync architecture this is less dangerous (the main thread is the sole writer, and snapshots only live on worker threads), but the invariant still holds: any state that snapshots need to read must live **inside Salsa** as an input or tracked query. URI-to-FileId resolution lives in `FilePathMap`; file content + metadata live in `FileEntryMap`. Snapshots access both through the `DbFiles` adapter, which only takes a `&dyn salsa::Database`.

**If you find yourself wanting to add an `Arc<RwLock<X>>` field to `AnalysisHost` that snapshots will also read, stop and put `X` in a Salsa input instead.** This was the root cause of #779, #784, #949.
