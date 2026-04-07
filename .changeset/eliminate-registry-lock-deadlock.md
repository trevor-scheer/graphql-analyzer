---
graphql-analyzer-lsp: patch
---

Eliminate the `FileRegistry` parking_lot `RwLock` from `Analysis` snapshots, which removes a class of LSP deadlocks triggered by rapid schema file edits. The previous fixes (#779, #784, #949) all worked around the same root cause: snapshots reached back into the host through a side-channel `RwLock` whose writer-blocks-readers semantics created a lock-ordering cycle with Salsa's setter/snapshot protocol. URI ↔ FileId resolution now lives in Salsa as a `FilePathMap` input, so snapshots resolve paths through `&db` and never share a non-Salsa lock with the host.
