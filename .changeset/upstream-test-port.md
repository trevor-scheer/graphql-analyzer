---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
---

Port `@graphql-eslint`'s unit tests verbatim into Rust unit tests under `crates/linter/src/rules/upstream/`, expanding lint-rule parity coverage from the existing single-fixture-per-rule integration test to upstream's full per-rule edge-case set. Surfaced and fixed multiple rule parity bugs along the way.
