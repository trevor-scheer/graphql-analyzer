---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
---

Port `@graphql-eslint`'s unit tests verbatim into Rust unit tests under `crates/linter/src/rules/upstream/`, expanding lint-rule parity coverage from the existing single-fixture-per-rule integration test to upstream's full per-rule edge-case set. Surfaced and fixed multiple rule parity bugs along the way.

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
