---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
---

Port `@graphql-eslint`'s unit tests verbatim into Rust unit tests under `crates/linter/src/rules/upstream/`, expanding lint-rule parity coverage from the existing single-fixture-per-rule integration test to upstream's full per-rule edge-case set. Surfaced and fixed multiple rule parity bugs along the way.

- `require-selections`: introduce `fieldName` option with OR semantics (any one of the listed names satisfies the requirement); add `requireAllFields: true` for AND semantics (one diagnostic per missing field); `fields` remains a deprecated alias for `fieldName`. This is a breaking change if you relied on `fields: [...]` requiring ALL listed fields.
- `require-nullable-fields-with-oneof`: extend checking to output object types with `@oneOf` (previously only input types were checked).
