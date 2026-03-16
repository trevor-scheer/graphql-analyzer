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
