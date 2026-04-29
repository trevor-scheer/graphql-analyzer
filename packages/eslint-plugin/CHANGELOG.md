# Changelog

All notable changes to `@graphql-analyzer/eslint-plugin` will be documented in
this file.
## 0.1.1 (2026-04-27)

### Features

- `@graphql-analyzer/eslint-plugin` is now a true drop-in replacement for `@graphql-eslint/eslint-plugin`. ESLint `rules: { rule: [severity, options] }` payloads now reach the analyzer; embedded GraphQL in JS/TS hosts is extracted by the processor with positions remapped back to the host file; multi-project `.graphqlrc.yaml` configs route per-file via `getProjectForFile`; all five upstream flat presets ship with byte-for-byte content; the 30 GraphQL spec validation rule names are exposed as no-op stubs so existing configs load cleanly. `naming-convention` and `alphabetize` gain schema-side enforcement and the bulk of upstream's options. ([#1025](https://github.com/trevor-scheer/graphql-analyzer/pull/1025))

### Fixes

- Graduate all packages from the `-alpha` prerelease line to stable. The previous `0.X.Y-alpha.0` GitHub releases captured the actual feature/fix content (browser playground, ESLint plugin parity, etc.); this release just drops the prerelease suffix so the next published versions are normal SemVer ([#1027](https://github.com/trevor-scheer/graphql-analyzer/pull/1027)).

## 0.1.1-alpha.0 (2026-04-26)

### Features

#### Initial release of `@graphql-analyzer/eslint-plugin` and the `@graphql-analyzer/core` native addon. ([#1002](https://github.com/trevor-scheer/graphql-analyzer/pull/1002))

- Drop-in replacement for `@graphql-eslint/eslint-plugin` — same plugin names, rule names, and flat-config preset names (`flat/schema-recommended`, `flat/operations-recommended`).
- Native performance via the Rust analyzer through a napi-rs binding.
- Configuration via `.graphqlrc.yaml` under `extensions.graphql-analyzer.lint`, with auto-discovery from the linted file's directory.
- Embedded GraphQL extraction from TypeScript, JavaScript, Vue, Svelte, and Astro.
- ESLint 8.40+ and ESLint 9.x supported (flat config only).

#### `@graphql-analyzer/eslint-plugin`: every shared lint rule is now verified end-to-end against `@graphql-eslint/eslint-plugin` with identical diagnostic counts, messages, and source positions. Behavior changes that align ours to graphql-eslint:

- **Message format** (backticks → double quotes around identifiers): `require-import-fragment`, `require-nullable-fields-with-oneof`, `strict-id-in-types`, `selection-set-depth`, `no-deprecated`, `require-deprecation-date`, and several rules touched by the alphabetize/option-schema work.
- **Diagnostic position**: `no-scalar-result-type-on-mutation`, `relay-connection-types`, `require-deprecation-reason`, and `require-deprecation-date` now point at the relevant type/directive name node (matching graphql-eslint) rather than the field name. `unique-enum-value-names` points at each duplicate value's name token. `require-selections` points at the SelectionSet `{`.
- **Firing condition**: `naming-convention` no longer applies hardcoded `OperationDefinition: PascalCase`/`FragmentDefinition: PascalCase`/`Variable: camelCase` defaults — the rule now no-ops without explicit kind config, matching graphql-eslint.
- **Option schemas**: `alphabetize`, `no-root-type`, `match-document-filename`, `selection-set-depth`, and `require-description` now accept the same option shapes graphql-eslint does (`maxDepth` instead of `max_depth`, kind-filter objects, etc.).
- **Semantics**: `require-deprecation-date` now reads the `@deprecated(deletionDate: "DD/MM/YYYY")` argument (rather than scanning the `reason` substring) and emits the same `MESSAGE_INVALID_FORMAT` / `MESSAGE_INVALID_DATE` / `MESSAGE_CAN_BE_REMOVED` diagnostics graphql-eslint does.
- **Multi-config support**: the napi host now resets per `init()` call, so monorepos with multiple `.graphqlrc.yaml` projects no longer leak schema/document state from one project into another.

#### **Drop-in name parity** with `@graphql-eslint/eslint-plugin`: the three remaining mismatched rule names were renamed so all 31 shared rules now line up 1:1.

- `unused-fields` → `no-unused-fields` (config key: `unusedFields` → `noUnusedFields`)
- `unused-fragments` → `no-unused-fragments` (config key: `unusedFragments` → `noUnusedFragments`)
- `unused-variables` → `no-unused-variables` (config key: `unusedVariables` → `noUnusedVariables`)

This is a breaking change for users who configured these rules under their old names; update `.graphqlrc.yaml` lint config keys accordingly. Migration guide added at `linting/migrating-from-graphql-eslint`.

The ESLint shim now propagates `messageId` and `fix` from the analyzer through to `LintMessage`. The parity test compares `(line, column, endLine, endColumn, message, messageId, fix)` together per diagnostic so any drift across rules surfaces as a clean diff. graphql-eslint emits stable `messageId` values for ~22 shared rules; those are now matched verbatim — both kebab-case ids that mirror the rule name (e.g. `"no-anonymous-operations"`, `"alphabetize"`) and the SHOUTY_SNAKE constants graphql-eslint uses for richer per-site distinctions (e.g. `"HASHTAG_COMMENT"`, `"MISSING_ARGUMENTS"`, `"MESSAGE_REQUIRE_DATE"`, `"MUST_HAVE_CONNECTION_SUFFIX"`).

Behavioral parity tightened on the three newly-aligned rules:

- **`no-unused-fields`** message now reads `Field "X" is unused` (matching graphql-eslint), with the diagnostic still anchored at the field name token.
- **`no-unused-fragments`** message reads `Fragment "X" is never used.` and the diagnostic anchors on the `fragment` keyword token (graphql-js's NoUnusedFragmentsRule range, post graphql-eslint adapter).
- **`no-unused-variables`** message reads `Variable "$name" is never used in operation "Op".` (or `… is never used.` for anonymous ops) and anchors on the `$` sigil — matching graphql-js verbatim.

The `alphabetize` rule now emits a `LintMessage.fix` matching graphql-eslint's swap edit. Other rules that ship internal autofixes (`require-selections`, `no-unused-fragments`, `no-unused-variables`) continue to expose those fixes to LSP/CLI consumers but suppress them in the ESLint shim, since graphql-eslint either ships them as `suggest` or doesn't autofix them at all.

### Fixes

- Extend `description-style` and `require-description` to cover nested AST nodes (fields, arguments, input values, enum values, directives) and — for `require-description` — operation definitions, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1011](https://github.com/trevor-scheer/graphql-analyzer/pull/1011)).
- `no-hashtag-description` diagnostics now report a single-position `loc` (start-only) when surfaced through `@graphql-analyzer/eslint-plugin`, matching `@graphql-eslint/eslint-plugin` exactly. The underlying analyzer diagnostic still carries the full comment range — that richness remains visible to the LSP and CLI; only the ESLint adapter strips the end position to mirror graphql-eslint's reporting shape.
- `require-nullable-result-in-root` now matches `@graphql-eslint/eslint-plugin` exactly: non-null list types like `[User!]!` are no longer flagged (only non-null *named* returns are), and the diagnostic message is `Unexpected non-null result <type> in type "<root>"` to match graphql-eslint's wording.
- `require-selections`: append `` or add to used fragment(s) `X` `` suffix when the missing field is reachable through fragments that don't contain it (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004))
- Type-position diagnostics for `require-field-of-type-query-in-mutation-result` and `require-nullable-result-in-root` now report at the field's return type name node, matching `@graphql-eslint/eslint-plugin` (closes part of [#1004](https://github.com/trevor-scheer/graphql-analyzer/issues/1004)) ([#1008](https://github.com/trevor-scheer/graphql-analyzer/pull/1008)).
