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

## P0 — Breaks the drop-in claim

### 1. Rule options passed via ESLint config aren't forwarded

**Status:** SOLVABLE.

**Evidence:** `packages/eslint-plugin/src/rules.ts:102` — `create(context)`
never reads `context.options`. `binding.ts:81-86` — `lintFile(filePath,
source)` takes no options. The parity test masks this:
`parity.test.mjs:664-670` writes options into `.graphqlrc.yaml` for us *and*
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
4. Update `parity.test.mjs` to pass options *only* via the ESLint rule entry
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
   wired and assert at least one diagnostic at the *embedded* line/column,
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

**Status:** SOLVABLE, but the largest single chunk of work in this list.

**Evidence:** `crates/linter/src/rules/naming_convention.rs` — only handles
`OperationDefinition`, `FragmentDefinition`, `VariableDefinition` (with
`Variable` alias). Missing options: `prefix`, `suffix`, `forbiddenPatterns`,
`requiredPattern`, `forbiddenPrefixes`, `forbiddenSuffixes`, `ignorePattern`,
`allowLeadingUnderscore`, `allowTrailingUnderscore`, the `types` umbrella,
all ESLint-style selector keys, and every schema-side kind
(`FieldDefinition`, `ObjectTypeDefinition`, `EnumValueDefinition`,
`InputValueDefinition`, `InterfaceTypeDefinition`, `UnionTypeDefinition`,
`ScalarTypeDefinition`, `EnumTypeDefinition`, `InputObjectTypeDefinition`,
`DirectiveDefinition`, `Argument`).

Parity test passes today only because both plugins no-op without explicit
kind config — a real upstream config silently under-covers when migrated.

**Plan:**

1. Implement the missing kinds first (schema-side selectors are the high
   value bit — most upstream configs target field and type names). Wire each
   to the appropriate HIR visitor.
2. Implement the option suite: `prefix`/`suffix`,
   `requiredPattern`/`forbiddenPatterns` (regex, with the same flag syntax
   upstream supports), `forbiddenPrefixes`/`forbiddenSuffixes`,
   `ignorePattern`, `allowLeadingUnderscore`/`allowTrailingUnderscore`.
3. Implement the `types` umbrella shorthand.
4. Add per-kind parity fixtures — one per selector — with options that
   exercise each option key. The fixture set is the contract; parity drift
   on any of them fails CI.

---

### 4. Autofix surface only covers `alphabetize`

**Status:** RESEARCH-FIRST, then SOLVABLE per-rule.

**Evidence:** `rules.ts:54-61` — `ESLINT_FIXABLE_RULES = new Set(["alphabetize"])`.
The doc claim at `eslint-plugin.mdx:158` ("Autofixes are not yet wired through
the ESLint rule shim") contradicts the fact that `alphabetize` *is* wired,
and the migration guide (`:101-104`) says the right thing.

**Plan:**

1. Enumerate which upstream rules ship `fix` (vs `suggest`-only) by reading
   each rule's source in `node_modules/@graphql-eslint/eslint-plugin/`.
   Produce a matrix in this file before any code changes.
2. For each upstream rule with `fix`: confirm our Rust rule produces an
   equivalent fix payload, add the analyzer rule name to
   `ESLINT_FIXABLE_RULES`, and add the rule's fixture to the parity-test set
   that compares `fix.range` and `fix.text`. The current `canonical()`
   already compares `fix` (`parity.test.mjs:631`) — we just need the
   fixtures to actually trigger fixes.
3. Reconcile the two docs (`eslint-plugin.mdx:158` and
   `migrating:101-104`) once the matrix is complete.

**Why research-first:** "fix" vs "suggest" is a per-rule editorial decision
upstream and we should match it exactly, not guess.

---

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

### 6. Missing `flat/recommended` catch-all preset

**Status:** SOLVABLE, trivial.

**Evidence:** `packages/eslint-plugin/src/configs.ts:21-24` only exports
`flat/schema-recommended` and `flat/operations-recommended`. Upstream ships a
combined `flat/recommended`; consumers importing it from us get an undefined
preset.

**Plan:** add `flat/recommended` to `configs.ts` as the union of the two
existing presets, with `files` constraints if needed (graphql-eslint's
`flat/recommended` may require a `files: ["**/*.graphql"]` block). Verify
against the upstream export shape before committing the structure.

---

### 7. `no-hashtag-description` per-line vs grouped granularity

**Status:** SOLVABLE.

**Evidence:** `eslint-plugin.mdx:163-165` documents the divergence. We fire
once per `#` comment line; upstream coalesces consecutive comment lines into
one diagnostic.

**Plan:** in `crates/linter/src/rules/no_hashtag_description.rs`, group
consecutive comment lines (comments on adjacent source lines, no
intervening blank line) and emit one diagnostic per run, located at the
first line of the run. Add a parity fixture with two adjacent `#` lines
followed by another after a blank line — should produce 2 diagnostics, not 3,
and match upstream's positions exactly. Then drop the caveat from the docs.

---

### 8. Multi-project `.graphqlrc` configs not supported

**Status:** RESEARCH-FIRST, then SOLVABLE or EXPLAIN.

**Evidence:** `binding.ts:53-79` — `ensureInitialized` resolves *one* config
file by walking parents, calls `coreBinding.init(resolved)` once per resolved
path, and never asks "which project does *this file* belong to". Documented
limitation in `migrating-from-graphql-eslint.mdx:99-100`.

**Plan:**

1. Confirm upstream's behavior: graphql-config supports `projects`
   natively, and graphql-eslint routes per-file to the matching project.
   Verify by reading their source (and their test fixtures with multi-project
   configs).
2. **If they do support it**: this is a real parity gap. Need a per-file
   project-resolution step (graphql-config's matchers) before calling the
   analyzer, plus a way to pass a project key into `lintFile` so the
   analyzer scopes its lookup. May require a Salsa input change to key
   projects within a config rather than per-config-file.
3. **If they don't**: drop the doc caveat; not a parity concern.

The Salsa init change is the largest unknown. If it turns out architecturally
disruptive (e.g. requires re-keying a lot of database inputs), document the
constraint here and ship without it — but only after confirming users won't
hit it on the common cases (single schema, multiple document globs is
already handled).

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
make sure the *exact* list in `KNOWN_MISSING` (`parity.test.mjs:49-83`)
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

A future "is this *really* the final parity PR?" check:

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
