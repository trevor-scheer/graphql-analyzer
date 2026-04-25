---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: minor
graphql-analyzer-mcp: minor
graphql-analyzer-core: minor
graphql-analyzer-eslint-plugin: minor
---

`@graphql-analyzer/eslint-plugin`: every shared lint rule is now verified end-to-end against `@graphql-eslint/eslint-plugin` with identical diagnostic counts, messages, and source positions. Behavior changes that align ours to graphql-eslint:

- **Message format** (backticks → double quotes around identifiers): `require-import-fragment`, `require-nullable-fields-with-oneof`, `strict-id-in-types`, `selection-set-depth`, `no-deprecated`, `require-deprecation-date`, and several rules touched by the alphabetize/option-schema work.
- **Diagnostic position**: `no-scalar-result-type-on-mutation`, `relay-connection-types`, `require-deprecation-reason`, and `require-deprecation-date` now point at the relevant type/directive name node (matching graphql-eslint) rather than the field name. `unique-enum-value-names` points at each duplicate value's name token. `require-selections` points at the SelectionSet `{`.
- **Firing condition**: `naming-convention` no longer applies hardcoded `OperationDefinition: PascalCase`/`FragmentDefinition: PascalCase`/`Variable: camelCase` defaults — the rule now no-ops without explicit kind config, matching graphql-eslint.
- **Option schemas**: `alphabetize`, `no-root-type`, `match-document-filename`, `selection-set-depth`, and `require-description` now accept the same option shapes graphql-eslint does (`maxDepth` instead of `max_depth`, kind-filter objects, etc.).
- **Semantics**: `require-deprecation-date` now reads the `@deprecated(deletionDate: "DD/MM/YYYY")` argument (rather than scanning the `reason` substring) and emits the same `MESSAGE_INVALID_FORMAT` / `MESSAGE_INVALID_DATE` / `MESSAGE_CAN_BE_REMOVED` diagnostics graphql-eslint does.
- **Multi-config support**: the napi host now resets per `init()` call, so monorepos with multiple `.graphqlrc.yaml` projects no longer leak schema/document state from one project into another.
