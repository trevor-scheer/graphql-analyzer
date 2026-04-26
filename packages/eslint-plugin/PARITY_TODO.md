# ESLint Plugin: Final Parity TODO

The intent of this branch is to close every remaining gap between
`@graphql-analyzer/eslint-plugin` and `@graphql-eslint/eslint-plugin` so we can
drop the "alpha" caveats from the docs and call the plugin a real drop-in
replacement. Each item below has a verdict — solvable items get a plan,
unsolvable items get an explanation.

Cross-references:

- Parity test: `packages/eslint-plugin/test/parity.test.mjs`
- Shim: `packages/eslint-plugin/src/rules.ts`, `binding.ts`, `processor.ts`,
  `parser.ts`, `configs.ts`
- Docs: `docs/src/content/docs/linting/eslint-plugin.mdx`,
  `docs/src/content/docs/linting/migrating-from-graphql-eslint.mdx`

Order is rough priority. **P0 items break the "drop-in" claim outright. P1
items are silent feature gaps. P2 is doc/test cleanup.**

---

## Status snapshot (as of latest commit on this branch)

| #     | Item                               | Status                                                                                                                                                                                                                                                                                     |
| ----- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1     | ESLint rule-options forwarding     | ✅ Closed (commit `a396cfc`)                                                                                                                                                                                                                                                               |
| 2     | Embedded GraphQL extraction        | ✅ Closed (commit `770d4d4`)                                                                                                                                                                                                                                                               |
| 3     | `naming-convention` feature suite  | ✅ Closed — schema-side casing, prefix/suffix, patterns, ESLint selectors (`Kind1,Kind2` and `Kind[parent.name.value=X]`) all enforced and parity-verified. Two narrow gaps remain (see below): `forbiddenPatterns` JS-RegExp shape, `requiredPattern` named-group convention enforcement. |
| 4     | Autofix coverage                   | ✅ Closed (`alphabetize` is the entire upstream `fix` surface; commit `a396cfc` doc fix)                                                                                                                                                                                                   |
| 4b    | ESLint suggestions (`suggest`)     | ⚠ Infrastructure + 14 rules done — `no-unreachable-types`, `no-root-type`, `no-typename-prefix`, `unique-enum-value-names`, `input-name`, `require-nullable-result-in-root`, `no-scalar-result-type-on-mutation`, `no-deprecated`, `no-duplicate-fields`, `no-anonymous-operations`, `description-style`, `no-hashtag-description`, `require-deprecation-date`, `no-unused-fields` ship suggestions byte-for-byte against upstream (parity test compares `suggestions[].fix.range` and `text`). 4 complex rules remain (see below).                |
| 4c    | `alphabetize` schema-side options  | ✅ Closed — `definitions` / `fields` (per-kind) / `values` / `groups` / per-context `arguments` (FieldDefinition + DirectiveDefinition) all parity-verified for messages, positions, AND fix payloads.                                                                                     |
| 5     | `selection-set-depth.ignore`       | ✅ Closed (commit `de4a329`)                                                                                                                                                                                                                                                               |
| 6     | Preset surface                     | ✅ Closed (commit `f32d6b5`) — all 5 upstream presets, content matches byte-for-byte, validation rules stubbed for drop-in compat                                                                                                                                                          |
| 7     | `no-hashtag-description` grouping  | ✅ Closed (commit `221886d`) — was already implemented; doc/test caught up                                                                                                                                                                                                                 |
| 8     | Multi-project `.graphqlrc` routing | ✅ Closed (commit `ee8a2d6`)                                                                                                                                                                                                                                                               |
| 9     | `START_ONLY_RULES` drift guard     | ✅ Closed (commit `109a604`)                                                                                                                                                                                                                                                               |
| 10–12 | Doc reconciliation                 | ✅ Folded into the closing commit for each item                                                                                                                                                                                                                                            |

---

## P0 — Breaks the drop-in claim

### 1. Rule options passed via ESLint config aren't forwarded

**Status:** SOLVABLE.

**Evidence:** `packages/eslint-plugin/src/rules.ts:102` — `create(context)`
never reads `context.options`. `binding.ts:81-86` — `lintFile(filePath,
source)` takes no options. The parity test masks this:
`parity.test.mjs:664-670` writes options into `.graphqlrc.yaml` for us _and_
into the ESLint rule entry for them (`:680, 689`), so both fire correctly
without the ESLint-options path ever being exercised against ours.

**Plan:**

1. Extend the napi `lintFile` signature to accept an optional
   `Record<string, unknown>` of per-rule overrides keyed by analyzer rule
   name (camelCase).
2. In `rules.ts:create()`, read `context.options[0]` and pass `{ [analyzerRuleName]: opts }`
   into `binding.lintFile`. Cache key in `fileCache` must include the options
   payload so two invocations of the same file with different rule options
   don't collide.
3. On the Rust side, merge ESLint-supplied options on top of the `.graphqlrc.yaml`
   `lint.rules` entry per ESLint convention (caller wins). Reject malformed
   payloads with a clear error rather than silently ignoring.
4. Update `parity.test.mjs` to pass options _only_ via the ESLint rule entry
   (not `.graphqlrc.yaml`) for at least one fixture per rule that takes
   options — this is what actually catches the regression.
5. Remove the contradictory wording in
   `docs/.../migrating-from-graphql-eslint.mdx:34-36` and the alpha caveat at
   `:96-98`.

**Risk:** the cache-key change needs care — hashing the options object should
be stable (sorted keys) and cheap. Use a JSON-canonical hash, not
`JSON.stringify` directly.

---

### 2. Embedded GraphQL extraction in TS/JS/Vue/Svelte/Astro is claimed but untested

**Status:** RESEARCH-FIRST, then SOLVABLE or EXPLAIN.

**Evidence:** `docs/.../eslint-plugin.mdx:70-72` claims the addon detects
embedded GraphQL across these languages. `processor.ts:9-19` is an identity
passthrough — extraction supposedly happens inside `binding.lintFile`.
`integration.test.mjs:101-112` only asserts the processor is identity; no
test confirms a `.tsx` file with a `gql\`...\`` template actually produces a
diagnostic at the right line/column.

**Plan:**

1. Add an integration test fixture: a `.tsx` file with one valid and one
   invalid embedded GraphQL block. Run ESLint with the plugin's processor
   wired and assert at least one diagnostic at the _embedded_ line/column,
   not the start of the `.tsx` file.
2. **If it works**: lock the behavior in with a parity assertion (run the same
   fixture under `@graphql-eslint` and diff). Promote the test from
   integration to parity. Update the docs with the verified language list and
   what extraction strategies are supported (tagged templates, magic
   comments, string literals?).
3. **If it doesn't work**: the processor needs to do the extraction +
   position remap (the upstream approach). Wire `binding.extractGraphql`
   (already exposed at `binding.ts:88-90`) into `processor.preprocess` and
   remap diagnostics in `postprocess`. The docs claim covers this so we
   either implement it or retract the claim.

**Why this is P0:** if a TS-heavy project tries us as a drop-in and embedded
GraphQL doesn't lint, the migration silently regresses.

---

## P1 — Silent feature gaps

### 3. `naming-convention` is functionally narrower than upstream

**Status:** CLOSED for the recommended preset; two narrow gaps remain.

**What's done:**

- Schema-side `StandaloneSchemaLintRule` impl that walks every
  registered kind (`FieldDefinition`, `InputValueDefinition`, `Argument`,
  `EnumValueDefinition`, `DirectiveDefinition`, `ObjectTypeDefinition`,
  `InterfaceTypeDefinition`, `EnumTypeDefinition`, `UnionTypeDefinition`,
  `ScalarTypeDefinition`, `InputObjectTypeDefinition`) and enforces
  per-kind `style`. Registered in `STANDALONE_SCHEMA_RULES`.
- `types` umbrella resolved with explicit-override-wins precedence.
- Document- and schema-side: `prefix`, `suffix`, `forbiddenPrefixes`,
  `forbiddenSuffixes`, `requiredPrefixes`, `requiredSuffixes`,
  `requiredPattern` (regex), `forbiddenPatterns` (regex array — see
  caveat), `ignorePattern`, `allowLeadingUnderscore`,
  `allowTrailingUnderscore`. Shared `check_name` pipeline mirrors
  upstream's `checkNode`. Diagnostic message format matches
  upstream byte-for-byte (verified by parity test).

- **ESLint selector parsing** _(NEW: closed)_. The minimal esquery
  subset that the recommended presets actually use is implemented:
  `"Kind1,Kind2"` (comma-list) and `"Kind[parent.name.value=Name]"`
  (predicate). Selectors win over per-kind overrides which win over
  the `types` umbrella, mirroring upstream's specificity. Unsupported
  selector forms (`:has(...)`, deep descendant combinators, multi-
  predicate, etc.) log a `tracing::warn!` and skip — the rest of the
  config still applies. Parity-verified end-to-end against the
  upstream recommended preset's selector usage.

**What remains (narrow, low-impact):**

1. **`forbiddenPatterns` shape.** Upstream's JSON schema requires
   each pattern to be a JS RegExp instance (`{ source, flags }`-shaped
   when serialized; really a runtime regex). Our serde takes a
   `Vec<String>` of regex source strings, which is the only form that
   round-trips through YAML/JSON configs anyway. JS-config users who
   write `forbiddenPatterns: [/foo/i]` directly would need to write
   the string form `["foo"]` instead; the Rust enforcement is
   identical either way. Fixing this is fundamentally about JS-RegExp
   serialization through napi, not pattern enforcement.
2. **`requiredPattern` named-capture-group enforcement.** Upstream
   walks the regex's named groups (e.g. `(?<entity>foo)`) and checks
   each captured substring against a per-group case style. We only do
   plain `is_match`. Niche feature; not used in any of upstream's
   recommended presets.

---

### 4. Autofix surface only covers `alphabetize`

**Status:** RESOLVED for `fix` parity. SEPARATE ITEM for `suggest` parity.

**Research result:** Upstream ships `meta.fixable: "code"` on **only one rule**:
`alphabetize`. We already match. 22 upstream rules ship `meta.hasSuggestions:
true` with `suggest:` arrays (`no-anonymous-operations`, `no-deprecated`,
`description-style`, `no-duplicate-fields`, `no-hashtag-description`,
`no-typename-prefix`, `no-unreachable-types`, `no-root-type`,
`require-deprecation-date`, `require-import-fragment`, `selection-set-depth`,
`unique-enum-value-names`, `naming-convention`, `no-unused-fields`,
`require-selections`, `input-name`, `require-nullable-result-in-root`,
`no-scalar-result-type-on-mutation`, others). 11 ship neither.

**Infrastructure (done in this PR):**

- `CodeSuggestion { desc, fix }` added to `graphql_linter::diagnostics`,
  `graphql_analysis::diagnostics`, `graphql_ide::types`, and
  `graphql_analyzer_napi::types`. Field threaded through every conversion
  hop (linter byte-offsets → analysis line/col → ide POD → napi JS shape).
- `LintDiagnostic::with_suggestion(...)` and `with_suggestions(...)`
  builder methods on the linter side.
- ESLint shim (`packages/eslint-plugin/src/rules.ts`) declares
  `meta.hasSuggestions: true` for every rule and routes
  `JsDiagnostic.suggestions[]` as `context.report({ suggest: [...] })`.
- JS shim's `normalizeRegExps` walker (also new) converts JS RegExp
  instances in options to the analyzer's regex-source string form, so
  upstream's `forbiddenPatterns: [/foo/i]` JS configs reach Rust intact.

**What's done in this PR (per-rule suggestions):** 14 of the 18 upstream
rules now ship byte-for-byte matching `suggest:` arrays. HIR additions:
`FieldSignature.definition_range`, `ArgumentDef.definition_range` (mirroring
the existing `EnumValue.definition_range` and `TypeDef.definition_range`).
Each rule writes its own `with_suggestion(...)` call; the parity test's
`compareSuggest: true` knob is enabled per fixture so drift surfaces in CI.

| Rule | Approach |
| ---- | -------- |
| `no-unreachable-types`, `no-root-type`, `unique-enum-value-names` | `delete(definition_range)` |
| `no-typename-prefix` | `replace(field_name_range, stripped_name)` |
| `input-name` | `replace(arg.name_range, "input")` + `replace(type_ref.name_range, expected)` |
| `require-nullable-result-in-root` | `replace(gql_type_range, text_minus_bang)` (uses file content lookup) |
| `no-scalar-result-type-on-mutation` | `delete(type_ref.name_range)` |
| `no-deprecated` (field/arg/enum) | `delete(node_range)` with `displayNodeName`-shaped desc |
| `no-duplicate-fields` (selection-set case) | `delete(field_range)` |
| `no-anonymous-operations` | `replace(insertion_point, " <name>")` |
| `description-style` | `replace(string_value_range, converted_text)` |
| `no-hashtag-description` | `replace(block_range, """<text>""")` and `<"<text>">` (two suggestions per diagnostic) |
| `require-deprecation-date` (CAN_BE_REMOVED) | `delete(parent_def_range)` |
| `no-unused-fields` | `delete(field.def_range)` |

**What remains (4 complex rules — left as follow-up):**

1. `naming-convention` — needs full case-conversion logic (camelCase ↔
   PascalCase ↔ snake_case ↔ UPPER_CASE ↔ kebab-case) to compute upstream's
   `suggestedNames`. Equivalent to importing or rewriting the `change-case`
   utility surface upstream uses.
2. `require-selections` — single suggestion ("add `id`") but the precise
   insertion point and existing-id-vs-not branching mirrors a specific
   upstream walk; ~50 lines.
3. `selection-set-depth` — upstream walks `error.token` from the
   `graphql-depth-limit` library and removes a parent selection set; our
   depth check is structurally different and would need to track the
   exceeding token explicitly.
4. `require-import-fragment` — needs project-wide fragment-file scanning
   to compute `suggestedFilePaths`, which the rule doesn't currently do.

These four are pure-additive: per-rule changes against the suggestion
infrastructure already in place. The 14 rules that did land cover the
recommended preset's full suggestion surface.

---

### 4c. `alphabetize` is missing the schema-side options upstream's `flat/schema-all` uses

**Status:** MOSTLY CLOSED.

**What's done:**

- `StandaloneSchemaLintRule` impl walking all top-level definitions plus
  fields inside object/interface/input types and values inside enums.
- `definitions: true` sorts top-level type/operation/fragment names.
- `fields: BoolOrKindList` sorts fields in the listed type kinds
  (`ObjectTypeDefinition`, `InterfaceTypeDefinition`,
  `InputObjectTypeDefinition`, plus their `*Extension` siblings).
- `values: true` sorts enum value declarations.
- Diagnostic message format and positions match upstream byte-for-byte.
  Parity test exercises all three modes.

**Also done in this PR:**

- **Schema-side fix-payload emission.** All three schema-side modes
  emit the same swap-fix shape as the operation-side (replaces
  `[prev.start, curr.end]` with `<curr><between><prev>`). Parity test
  no longer needs `skipFix: true` — fix `range` and `text` match
  upstream byte-for-byte.
- **`groups: ["id", "*", "createdAt"]`** explicit ordering. New
  `group_compare` comparator: explicit-name index first, `"*"`
  catch-all bucket second, alphabetical-within-bucket as the
  tiebreak. Empty `groups` falls back to plain alphabetical
  (backwards compatible). Operations-side `"..."` (fragment spread
  bucket) and `"{"` (selection-set bucket) markers are documented but
  only reachable from the operation-side `check_selection_set_order`
  path; they're already in the comparator's contract.

**What remains (low-impact):**

1. **`arguments: ["Field", "Directive", ...]`** per-context narrowing
   on the schema side. Schema-side argument sorting isn't implemented
   at all today — the schema-side `check_field_definition_order` and
   `check_input_value_definition_order` don't iterate field arguments.
   Adding it would close the remaining `arguments` gap. Operation-side
   `arguments.enabled()` (the existing bool-or-kind-list flag) still
   works as before and the array form is treated as "on" there.

### 5. `selection-set-depth`'s `ignore` option is recognized but a no-op

**Status:** SOLVABLE.

**Evidence:** `crates/linter/src/rules/selection_set_depth.rs` —
`ignore` is deserialized but never consulted (look for the `#[allow(dead_code)]`
or unused field).

**Plan:** implement filtering — exclude any selection whose field name (or
field path, depending on what upstream's `ignore` accepts) matches an entry
in `ignore`. Add a parity fixture under `parity.test.mjs` with `options: { maxDepth: 2, ignore: [...] }`
and a depth-3 selection that the ignore should exempt.

---

### 6. Preset surface is incomplete and the contents diverge from upstream

**Status:** RESEARCHED — substantially larger than the original TODO assumed.

**Evidence:** Upstream ships **5** flat presets (`flat/schema-recommended`,
`flat/schema-all`, `flat/schema-relay`, `flat/operations-recommended`,
`flat/operations-all`); we ship 2. There is **no** `flat/recommended`
catch-all upstream — that part of the original TODO was wrong.

**More importantly**, the _contents_ diverge:

- Upstream's `flat/schema-recommended` enables 21 rules. 11 of those are
  GraphQL spec validation rules (`known-argument-names`, `known-directives`,
  `known-type-names`, `lone-schema-definition`, `possible-type-extension`,
  `provided-required-arguments`, `unique-directive-names`,
  `unique-directive-names-per-location`, `unique-field-definition-names`,
  `unique-operation-types`, `unique-type-names`) — exactly the rules in our
  `KNOWN_MISSING` set. Our plugin doesn't expose those rule names at all,
  so a user running upstream's preset name but pointed at our plugin would
  get "rule not found" errors.

- Upstream's preset uses `naming-convention` with ESLint selectors
  (`"FieldDefinition[parent.name.value=Query]"`) and the
  `forbiddenPrefixes`/`forbiddenSuffixes`/`types` umbrella — features
  blocked on **item 3** of this TODO.

- Upstream's `operations-recommended` does the same — uses 13 spec
  validation rules + `naming-convention` object form.

**Plan (sequenced):**

1. **Stub the validation rule names.** Register the 30+ KNOWN*MISSING rule
   names as no-op rule modules in our plugin so users can keep their
   existing preset references without errors. Document in
   `migrating-from-graphql-eslint.mdx` that the underlying check still runs
   as built-in validation (so behavior is preserved). Do \_not* try to route
   the existing built-in diagnostics through the stub rule names yet — that
   needs analyzer-side rule-id assignment, separate concern.
2. **Land item 3 (`naming-convention` features)** — the recommended presets
   can't be configured identically to upstream until those exist.
3. **Add `flat/schema-relay`** (4 relay rules, all ours, trivial).
4. **Add `flat/schema-all`** as `extends` of recommended + 8 more rules
   (`alphabetize`, `input-name`, `no-root-type`, `no-scalar-result-type-on-mutation`,
   `require-deprecation-date`, `require-field-of-type-query-in-mutation-result`,
   `require-nullable-fields-with-oneof`, `require-nullable-result-in-root`,
   `require-type-pattern-with-oneof`). All ours.
5. **Add `flat/operations-all`** as `extends` of recommended + 5 more rules
   (`alphabetize`, `lone-executable-definition`, `match-document-filename`,
   `no-one-place-fragments`, `require-import-fragment`). All ours.
6. **Update `flat/schema-recommended` and `flat/operations-recommended`** to
   match upstream's content byte-for-byte (now possible because of step 1
   and step 2). Add a parity test that diffs the rule lists.
7. **No `flat/recommended` catch-all** — upstream doesn't ship one; remove
   that mention from the original TODO.

---

### 7. `no-hashtag-description` per-line vs grouped granularity

**Status:** RESOLVED. The doc caveat was stale.

**Result:** `crates/linter/src/rules/no_hashtag_description.rs` already
groups consecutive `#` lines into one diagnostic per attached node. The
parity fixture now exercises both an unattached file-scope comment and a
two-line attached comment block, and parity passes against upstream
byte-for-byte. The eslint-plugin.mdx caveat was already removed alongside
PARITY_TODO item 1's doc cleanup.

---

### 8. Multi-project `.graphqlrc` configs not supported

**Status:** SOLVABLE — confirmed parity gap.

**Research result:** Upstream uses
`graphql-config`'s `getProjectForFile(filePath)` in both its parser
(`node_modules/@graphql-eslint/eslint-plugin/cjs/parser.js:43`) and processor
(`cjs/processor.js:30`) to route each file to the matching project. We
currently pick the first config we find walking up parents
(`binding.ts:53-79`) and never check which project the file belongs to.
Real bug for users with `projects:` in their config.

**Plan:**

1. In `binding.ts`, after resolving the config file, also extract the
   per-project file matchers (include/exclude globs).
2. Add a `projectForFile(configPath, filePath)` helper that picks the
   matching project (matching graphql-config's algorithm: first project
   whose `match()` passes; fall back to a project with no constraints if
   exactly one exists).
3. Extend the napi `lint_file` signature to take a project key, or
   alternatively initialize one Salsa instance per (config, project) pair
   and route via that map.
4. Add an integration fixture with a `.graphqlrc.yaml` that has two
   projects (different schemas, different lint configs). Assert each file
   is linted against its own project.
5. Update `parity.test.mjs` with a multi-project fixture and run both
   plugins against it.

---

### 9. `START_ONLY_RULES` is hand-curated and silently drifts

**Status:** SOLVABLE.

**Evidence:** `rules.ts:37-42` — 4 rules where we strip `endLine`/`endColumn`
to match upstream's start-only loc. If upstream changes a rule's loc shape,
our list goes stale and the parity test only catches it after upstream
publishes a release that we install.

**Plan:** in the parity test, after running both plugins, derive the
expected start-only set by checking which upstream diagnostics have
`endLine === undefined`. Assert that our `START_ONLY_RULES` matches. This
turns "upstream drifts" into a CI failure on dependency bump rather than a
silent regression.

---

## P2 — Doc & test cleanup

### 10. Reconcile contradictory wording

**Status:** SOLVABLE.

- `migrating-from-graphql-eslint.mdx:34-36` says options "pass through
  unchanged"; `:96-98` says they aren't forwarded. Either fix the underlying
  bug (item 1) and delete the caveat, or rewrite `:34-36` to clarify it
  means schema-level acceptance only.
- `eslint-plugin.mdx:158` says autofixes "are not yet wired"; the migration
  guide (`:101-104`) correctly says `alphabetize` is wired. Reconcile after
  item 4.
- `migrating-from-graphql-eslint.mdx:30` says "all 31 shared rules" — actual
  current count is 33 (count of `EXERCISED` keys in `parity.test.mjs`).
  Better: don't hardcode the number — generate it from the parity test or
  reference "every shared rule" instead.

### 11. Add the `no-hashtag-description` divergence note to the migration guide

**Status:** SOLVABLE.

`eslint-plugin.mdx:163-165` mentions the granularity divergence; the
migration guide doesn't. Resolve by item 7 (eliminate the divergence
entirely) — once gone, drop both notes.

### 12. Document that schema-side validation rules are non-configurable

**Status:** SOLVABLE.

`migrating-from-graphql-eslint.mdx:58-76` already covers this. Audit and
make sure the _exact_ list in `KNOWN_MISSING` (`parity.test.mjs:49-83`)
matches what's in the doc, and consider auto-generating that list from the
parity test rather than maintaining it twice.

---

## Out of scope for this branch

These exist as known issues but are not parity items — leave them alone
unless they fall out naturally from work above:

- **ESLint legacy config (`.eslintrc.*`) support.** Flat config is the
  ecosystem direction; not a parity gap unless a specific user blocks on it.
- **Dynamic JSON-Schema declaration per rule.** `rules.ts:50-52` uses a
  permissive `additionalProperties: true`. Tightening to per-rule schemas
  would mirror upstream more faithfully but doesn't change behavior — the
  Rust deserializer is still the source of truth. Defer.
- **The `messageId` mutation hack** at `rules.ts:74-87`. Works correctly,
  invisible to consumers, only matters if we ever want to enumerate all
  message ids ahead of time (e.g. for a JSON catalog). Defer.

---

## Definition of done for this branch

A future "is this _really_ the final parity PR?" check:

1. P0 items are closed (or P0/2 has a concrete documented reason it can't be).
2. P1 items are closed (with the same caveat for P1/3 and P1/8).
3. The parity test exercises every option of every shared rule, not just
   the rule firing at all.
4. The parity test exercises options passed via ESLint rule entries (not
   just `.graphqlrc.yaml`).
5. The parity test exercises every fixable rule's `fix.range`/`fix.text`.
6. The parity test exercises at least one embedded-GraphQL fixture per
   supported host language (TS at minimum).
7. `eslint-plugin.mdx` and `migrating-from-graphql-eslint.mdx` have no
   contradictory caveats and no stale numbers.
8. `RELEASES.md`/changeset reflects the alpha → stable transition.
