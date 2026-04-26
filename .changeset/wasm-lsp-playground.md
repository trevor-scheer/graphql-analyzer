---
graphql-lsp: minor
graphql-lsp-wasm: minor
@graphql-analyzer/web-ide: minor
---

Add browser playground (`@graphql-analyzer/web-ide`) backed by a wasm build of the language server. The LSP can now compile to `wasm32-unknown-unknown` via the new `graphql-lsp-wasm` crate.
