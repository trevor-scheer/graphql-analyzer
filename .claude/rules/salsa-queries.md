---
description: Salsa query patterns and cache invariants for the database layer
paths:
  - "crates/graphql-db/**"
  - "crates/graphql-hir/**"
---

# Salsa Query Patterns

## Cache Invariants

These invariants MUST be preserved for incremental computation to work:

| Invariant                     | Meaning                                                       |
| ----------------------------- | ------------------------------------------------------------- |
| **Structure/Body separation** | Editing body content never invalidates structure queries      |
| **File isolation**            | Editing file A never invalidates unrelated queries for file B |
| **Index stability**           | Global indexes stay cached when edits don't change names      |
| **Lazy evaluation**           | Body queries only run when results are needed                 |

**Structure** = identity (names, types). **Body** = content (selection sets, directives).

## Query Design Rules

- Never store `FileId` in a query result if the query doesn't depend on that file
- Use `#[salsa::tracked]` for computed values that should be memoized
- Use `#[salsa::input]` only for external data (file contents, config)
- Prefer fine-grained queries over coarse ones to maximize cache reuse
