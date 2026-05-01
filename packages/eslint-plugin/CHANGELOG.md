# Changelog

All notable changes to `@graphql-analyzer/eslint-plugin` will be documented in
this file.
## 0.1.4 (2026-05-01)

### Features

- Remove the `require-id-field` lint rule — strict subset of `require-selections` with cosmetic differences. Migrate `requireIdField: <severity>` to `requireSelections: [<severity>, { requireAllFields: true }]` (or pass the same `fields:` list as `fieldName:` if you customised it) ([#1083](https://github.com/trevor-scheer/graphql-analyzer/pull/1083))

### Fixes

- `require-selections`: emit one quick-fix suggestion per missing `idName` instead of a single autofix that stacks every candidate. Picking which `idName` to add is a semantic choice; the IDE menu now offers one entry per candidate, matching `@graphql-eslint`. The single-candidate case still autofixes ([#1079](https://github.com/trevor-scheer/graphql-analyzer/pull/1079))

## 0.1.3 (2026-05-01)

### Features

- Document `.vue`, `.svelte`, and `.astro` host support in the ESLint plugin and add end-to-end tests proving embedded-GraphQL extraction (and autofix range remapping) works for all three SFC formats ([#1036](https://github.com/trevor-scheer/graphql-analyzer/issues/1036))

## 0.1.2 (2026-04-30)

### Features

- Add `resty-field-names` lint rule to detect REST anti-patterns ([#930](https://github.com/trevor-scheer/graphql-analyzer/pull/930))

### Fixes

#### Port `@graphql-eslint`'s unit tests verbatim into Rust unit tests under `crates/linter/src/rules/upstream/`, expanding lint-rule parity coverage from the existing single-fixture-per-rule integration test to upstream's full per-rule edge-case set. Surfaced and fixed multiple rule parity bugs along the way.

- `alphabetize`: locale-aware comparison so lowercase sorts before uppercase when names are case-insensitively equal (matches JS `localeCompare` en-US behavior); inline-fragment sentinel in selection ordering — a named field that should sort before an inline fragment now fires unconditionally; `{` and `...` group buckets are recognized in the `groups` option for selection ordering; single-pass depth-first recursion changes the diagnostic emission order to match upstream; `InputValueDefinition` arguments are now labeled `"input value"` (was `"argument"`); operation-type labels (`query`/`mutation`/`subscription`/`schema definition`) are tracked in definitions ordering for anonymous-node handling.

- `description-style`: now emits `messageId: "description-style"` on diagnostics, required for correct ESLint shim consumption.

- `lone-executable-definition`: the `ignore` option is now implemented and accepted.

- `match-document-filename`: `UPPER_CASE` style is now recognized; the `prefix` option is now implemented; a plain string is now accepted as shorthand for a `DefinitionTypeConfig`; the extension check now fires on anonymous operations in addition to named ones.

- `naming-convention`: `gqlType.name.value` and `gqlType.gqlType.name.value` selector predicates are now supported; the `!=` predicate operator is now supported alongside `=`; ESLint `[{...}]` array-wrapped options are now unwrapped automatically; `forbiddenPatterns` regex values are now displayed in `/pattern/flags` format matching upstream.

- `no-deprecated`: bare `@deprecated` with no explicit `reason` argument now fires with the GraphQL spec default reason "No longer supported" (was silently skipped); input object field deprecation is now checked when an argument value is an object literal; non-string `@deprecated(reason: ...)` values (e.g. numeric literals) are now stringified at the HIR level so the rule no longer incorrectly fires on them.

- `no-duplicate-fields`: duplicate variable definitions and duplicate arguments within an operation are now also caught (previously only duplicate field selections were reported).

- `no-one-place-fragments`: fragment spread occurrence count is now the raw spread count across the document rather than the count of unique containing definition sites. A fragment spread twice within one operation counts as 2 uses.

- `no-unreachable-types`: unreachable directive definitions are now reported (built-in directives are always skipped); directive argument types are tracked for reachability — types referenced only by directive arguments with executable locations are considered reachable; a reverse-implementors pass means when an interface becomes reachable, all types implementing it become reachable too; unreachable scalars are now reported (were unconditionally excluded before); per-file diagnostics are sorted by source position; per-declaration error counts for type extensions via a new `schema_utils::raw_schema_type_defs()` helper.

- `no-unused-fields`: `ignoredFieldSelectors` option added — accepts `[parent.name.value=X][name.value=Y]` selectors (with `/regex/` support) to skip Relay pagination boilerplate fields; `skipRootTypes` option added — defaults to `true` to preserve existing behavior, set to `false` to check root type fields (required for full upstream parity).

- `no-unused-variables`: cross-file fragment variable usage is now tracked — a variable used inside a fragment defined in another file is no longer incorrectly reported as unused.

- `relay-arguments`: built-in scalars (`Int`, `Float`, `String`, `Boolean`, `ID`) are now recognized as valid types for `after`/`before` cursor arguments (previously only user-defined scalars were accepted).

- `relay-connection-types`: list-wrapped `pageInfo` (e.g. `pageInfo: [PageInfo]!`) is now rejected; per-declaration error counts for type extensions.

- `relay-edge-types`: non-Object edge types (scalar, union, enum, interface) now emit `MUST_BE_OBJECT_TYPE` instead of being silently skipped; `listTypeCanWrapOnlyEdgeType` now scans every Object and Interface type in the schema rather than only connection types.

- `relay-page-info`: per-declaration error counts for type extensions.

- `require-deprecation-date`: type-level `@deprecated` directives (e.g. `scalar Old @deprecated`) are now checked in addition to field, enum value, and argument directives.

- `require-deprecation-reason`: empty (`""`) and whitespace-only (`"  "`) `reason` strings are now treated as missing (mirrors upstream's `.trim()` check); type-level `@deprecated` directives are now checked in addition to field, enum value, and argument directives.

- `require-description`: per-kind type flags added (`ObjectTypeDefinition`, `InterfaceTypeDefinition`, `EnumTypeDefinition`, `ScalarTypeDefinition`, `InputObjectTypeDefinition`, `UnionTypeDefinition`) — each accepts a boolean that overrides the umbrella `types` flag when set; `rootField` option implemented to fire on undescribed fields of root operation types independently of `FieldDefinition`; `ignoredSelectors` option implemented using `[type=Kind][name.value=X]` attribute-selector syntax with `/regex/` support for the name value.

- `require-import-fragment`: default imports (`# import 'path'`) are now supported and validated against project files; path-based fragment-presence validation for named imports — the referenced file must actually contain the named fragment when the file is present in the project.

- `require-nullable-fields-with-oneof`: checking is now extended to output object types with `@oneOf` (previously only input types were checked). A non-null field on `type Foo @oneOf` now produces a diagnostic.

- `require-selections`: `fieldName` option introduced with OR semantics — any one of the listed field names satisfies the requirement per selection set; `requireAllFields: true` option added for AND semantics with one diagnostic per missing field; `fields` is kept as a deprecated alias for `fieldName`. **Breaking change**: the previous `fields: [...]` behavior required ALL listed fields simultaneously; that AND semantics is now only available via `requireAllFields: true`.

- `selection-set-depth`: cross-file fragment spreads are now inlined when computing depth, so a spread into a fragment defined in a sibling file contributes to the depth count correctly.

- All lint rules: `# eslint-disable-next-line <rule>`, `# eslint-disable <rule>`, and `# eslint-enable <rule>` directive comments in `.graphql` files are now honored across the full production lint pipeline (LSP, CLI, MCP). Previously these directives were parsed only in the upstream test harness and had no effect on real user output.

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
