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

## Async Safety: DashMap and Locks

**Never hold a `DashMap` `Ref` (or any non-async lock guard) across an `.await` point.**

`DashMap::get()` returns a `Ref<'_, K, V>` that holds a `parking_lot` shard read lock. If you hold this `Ref` as a local variable and then call `.await`, the current thread suspends while owning the lock. If another task on the Tokio runtime tries to acquire the same shard's write lock (e.g., via `DashMap::entry()`), it blocks. With a saturated thread pool, nothing can resume the suspended task to release the lock — **deadlock**.

**Rules:**

- Always call `.map(|r| r.clone())` on a `DashMap` reference before any `.await`
- Prefer typed accessor methods on `WorkspaceManager` (e.g., `get_host`, `all_hosts`, `projects_for_workspace`) over direct field access — these enforce the clone-before-await contract by returning owned values
- The `hosts` field on `WorkspaceManager` is private for this reason; all callers must go through the typed API
- Same applies to `std::sync::Mutex`, `parking_lot::RwLock`, and any other non-`tokio::sync` lock type

## Snapshot/Host Lock Discipline

**`Analysis` snapshots and `AnalysisHost` MUST NOT share any non-Salsa lock.**

Salsa setters block until all outstanding `db.clone()` snapshots are dropped. If a snapshot reaches back into the host through any other lock (parking_lot, std::sync, tokio::sync — doesn't matter), you have a lock-ordering cycle waiting to happen:

1. Snapshot holds a Salsa db clone, acquires the side-channel lock for a lookup.
2. Host writer wants the side-channel lock for write, parks behind the snapshot.
3. Eventually the snapshot tries to reacquire the side-channel lock for its next lookup; with a writer queued, it parks too.
4. Writer's Salsa setter is now waiting on the snapshot to drop. Snapshot is waiting on the writer to release the lock. **Deadlock between two `spawn_blocking` workers.**

The fix is structural: any state that snapshots need to read must live **inside Salsa** as an input or tracked query, not behind a side-channel lock. URI ↔ FileId resolution lives in the `FilePathMap` Salsa input (`crates/base-db/src/lib.rs`); file content + metadata live in `FileEntryMap`. Snapshots access both through the `DbFiles` adapter (`crates/ide/src/db_files.rs`), which only takes a `&dyn salsa::Database`. There is no `FileRegistry` reference, no `Arc<RwLock<...>>`, nothing the snapshot can park on except Salsa itself.

**If you find yourself wanting to add an `Arc<RwLock<X>>` field to `AnalysisHost` that snapshots will also read, stop and put `X` in a Salsa input instead.** This was the root cause of #779, #784, #949, and the architectural fix that finally killed the class. The historical incident is documented in the `salsa.md` SME agent under "Pitfall 2".

The `tokio::sync::Mutex<AnalysisHost>` in `ProjectHost` is fine and required — it serializes writers and is on the async side, so snapshots never touch it. The bad pattern is a _parking_lot_ (or any sync) lock that both sides hold.
