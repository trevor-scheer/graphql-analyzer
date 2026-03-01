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

| Crate         | Purpose                                    |
| ------------- | ------------------------------------------ |
| `base-db`     | Salsa database foundation, FileId          |
| `syntax`      | GraphQL parser, TS/JS extraction           |
| `hir`         | Semantic layer, structure/body separation  |
| `analysis`    | Validation and diagnostics                 |
| `ide-db`      | IDE-specific database queries              |
| `ide`         | IDE features (hover, completion, goto-def) |
| `linter`      | Lint rules and configuration               |
| `lsp`         | Language Server Protocol implementation    |
| `cli`         | Command-line interface                     |
| `mcp`         | Model Context Protocol server              |
| `config`      | Project configuration (.graphqlrc.yaml)    |
| `introspect`  | Remote schema introspection via HTTP       |
| `extract`     | GraphQL extraction from TS/JS files        |
| `apollo-ext`  | Apollo-specific extensions                 |
| `types`       | Shared type definitions                    |
| `test-utils`  | Testing utilities and fixtures             |

---

## GraphQL Document Model

**Fragment scope is project-wide**, not file-scoped:

- Operations can reference fragments in other files
- Fragment spreads can reference other fragments (transitive dependencies)
- Fragment and operation names must be unique across the entire project

**When validating operations**, you MUST:

1. Include direct fragment dependencies
2. Recurse through fragment dependencies
3. Handle circular references
4. Validate against schema for all fragments in the chain

---

## Cache Invariants

The Salsa architecture relies on these invariants for incremental computation:

| Invariant                     | Meaning                                                       |
| ----------------------------- | ------------------------------------------------------------- |
| **Structure/Body separation** | Editing body content never invalidates structure queries       |
| **File isolation**            | Editing file A never invalidates unrelated queries for file B  |
| **Index stability**           | Global indexes stay cached when edits don't change names       |
| **Lazy evaluation**           | Body queries only run when results are needed                  |

**Structure** = identity (names, types). **Body** = content (selection sets, directives).

---

## Protected Core Features

These features must NOT be removed or degraded:

| Feature                       | Why Critical                      | What Enables It                                        |
| ----------------------------- | --------------------------------- | ------------------------------------------------------ |
| **Embedded GraphQL in TS/JS** | Most users write queries in TS/JS | `documentSelector` includes TS/JS in VS Code extension |
| **Real-time diagnostics**     | Users expect immediate feedback   | LSP `textDocument/didChange` notifications             |
| **Project-wide fragments**    | Fragments span many files         | `all_fragments()` indexes entire project               |

**Solve performance problems without removing features.** Use filtering, lazy evaluation, debouncing, or configuration options instead.
