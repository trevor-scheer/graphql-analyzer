---
description: LSP handler conventions and protocol compliance
globs:
  - "crates/graphql-lsp/**"
---

# LSP Handler Conventions

- All request handlers go in `crates/graphql-lsp/src/handlers/`
- Register new capabilities in the server initialization
- Every handler must support cancellation via the LSP cancellation token
- Return `None`/empty results rather than errors for graceful degradation
- Prefer incremental document sync over full sync
- Never block the main message loop with expensive computation
