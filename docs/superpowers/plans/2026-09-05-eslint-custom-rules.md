# GraphQL ESLint custom-rule compatibility

## Objective and acceptance criteria

Implement the documented GraphQL-ESLint v4 custom-rule contract in `@graphql-analyzer/eslint-plugin`, with independently verified behavior and an explicit native-only performance option. ESLint remains responsible for registering and executing third-party JavaScript rules. The Rust CLI and LSP do not acquire a JavaScript runtime.

The default `parser` supplies a GraphQL ESTree tree. `fastParser` preserves the empty-Program path for native rules. The corresponding `processor` supports embedded custom rules; `fastProcessor` retains the original-source path. No contrary public-API preference was received during planning, so this is the implementation default.

Acceptance requires custom visitors, selectors and exit listeners; graphql-js-compatible `rawNode()` and `typeInfo()`; schema and sibling-operation services; public rule types and schema/sibling helpers; ordinary ESLint RuleTester support; embedded reports, fixes and suggestions; preserved host-language linting; and reproducible performance comparisons. Dependencies on graphql-js and schema/config loaders are acceptable. Re-exporting the upstream plugin as the implementation is not the proposed full build.

Scope is the custom-rule frontend contract. Existing native built-in limitations, including ESLint rule-option forwarding and native multi-project/inline config support, are not claimed fixed by exposing compatible parser services. Document the distinction explicitly and test mixed custom/native configurations within the native engine's supported config scope.

## Evidence and corrections to the initial discussion

- `packages/eslint-plugin/src/parser.ts` returns an empty Program and ignores parser options.
- `packages/eslint-plugin/src/rules.ts` shares a native invocation via a process-wide path/content cache. One native call does not imply one AST traversal: Rust invokes individual rules separately.
- `crates/syntax/src/lib.rs` builds both an apollo-parser CST and an apollo-compiler AST. The native engine already has multiple parse representations.
- `crates/napi/src/lib.rs` exposes diagnostics and extracted blocks, but no AST or graphql-js schema. A native ESTree transport would be a new API, not simply serialization of an existing JS-ready tree.
- `crates/napi/src/host.rs` skips updating known files and resets its sole host on config initialization. Both behaviors need regression coverage before relying on native diagnostics during autofix or mixed-project runs.
- `packages/eslint-plugin/src/binding.ts` remembers initialized config paths even though the native host is replaced. The diagnostic cache also lacks project dependency invalidation.
- Native extracted offsets are UTF-8 byte offsets. ESLint ranges use UTF-16 code units. Cooked strings and interpolations require more than adding a constant offset.
- There are no measured ESLint speedup numbers in this investigation. Additional AST allocation, traversal, and schema construction are expected costs; their sizes require measurement.

## Design decisions to pressure-test

### Parser and conversion

Build the compatibility frontend in the plugin, using graphql-js parsing and an owned ESTree converter. Match upstream node fields, including renaming GraphQL `type` child fields to `gqlType`, raw-node identity, descriptions/leading comments, locations, tokens, comments, Program wrapping, and visitor keys. Convert parsing failures to ESLint locations. Empty/comment-only documents need upstream comparison.

Start with an independently correct JS implementation because graphql-js raw nodes and schema classes are part of the public contract. Native CST/AST export is an optimization candidate, not a prerequisite for custom-rule execution. Do not claim parse reuse or unchanged schema cost. A future native transport must prove both compatibility and a net performance benefit over the JS parse it replaces.

Compute type information once per document on first demand and cache snapshots per raw node, if this preserves upstream behavior. Returning a live mutable TypeInfo object is incorrect. Syntax-only rules should not allocate type snapshots unnecessarily.

### Schema, documents, and public API

Use `graphql-config` synchronous loading and graphql-tools sync loaders where compatible with ESLint's synchronous parser contract. Respect `parserOptions.graphQLConfig`, `schemaSdl`, physical config discovery, project selection, and missing-schema errors. Provide genuine graphql-js schema objects and sibling document ASTs. Verify loader behavior against the installed upstream v4 implementation before choosing cache lifetime.

Export owned `GraphQLESLintRule`, node/context/service types, `requireGraphQLSchema`, and `requireGraphQLOperations`. Test imports from the built package, not private source files. Upstream 4.4.0 exports `requireGraphQLOperations` and no `GraphQLRuleTester`; use ordinary ESLint RuleTester and avoid an unnecessary public wrapper. Keep ESLint as a peer dependency; graphql-js should be a peer to preserve schema class identity. Do not ship the upstream plugin as a runtime dependency.

Make both compatibility entry points lazy wrappers so importing the root plugin and using `fastParser`/`fastProcessor` does not load graphql-js/config/pluck. Type-only exports must stay erased. Verify this in a fresh process using module load tracking; use a separate fast entry point only if lazy wrappers prove insufficient.

### Embedded documents and native rules

Return the host source plus named `.graphql` virtual documents. Keep a precise per-block source mapping for diagnostic starts/ends, fixes, and suggestions. Preserve host parser behavior and distinguish virtual filenames from physical filenames. Extraction and mapping must handle multiple blocks, indentation, CRLF, Unicode, template interpolations, and escaped strings. Reject or suppress fixes that cannot be mapped safely; never corrupt surrounding source.

Use `@graphql-tools/graphql-tag-pluck` for the compatibility processor and retain native extraction for the fast path. The native extractor rejects interpolated templates and cannot currently meet the compatibility contract. Build a source segment map from plucked bodies and original literals; use exact substring mapping where available, and conservatively suppress fixes over transformed or ambiguous spans. A simple offset-only implementation is insufficient for cooked strings. Inspect and test installed upstream behavior instead of promising support based on file extensions alone.

Native rule shims need physical-file context while reporting in virtual-document coordinates. The processor retains a per-preprocess record of physical source, virtual blocks, mappings, and lazily computed physical diagnostics; rule shims look up that record, filter diagnostics by block, and translate into virtual coordinates before postprocessing. Avoid adding virtual files to the native project's schema/document inventory or running the same native diagnostic twice. Scope shared diagnostics to one ESLint SourceCode using a WeakMap for standalone files, and to the processor record for blocks. Release records in postprocess and on failed preprocessing. Preserve direct native linting of JS/TS for fast mode.

### Native correctness prerequisites

Add failing tests, then fix known-file source updates and config switching in separate commits. Preserve schema/executable classification during edits and refresh changed on-disk dependencies without overwriting unchanged unsaved files. Replace remembered config-path sets with actual active-config state and retry failed/missing config discovery. Reinitialization on config switches is acceptable for correctness; do not claim incremental reuse across switches. Bound changes to what is required for repeat linting, custom-rule autofix, and processor integration. Do not silently expand this PR to all unrelated migration gaps.

## Commit and delegation plan

1. Commit the reviewed plan and a small reproducible compatibility baseline.
2. Native owner: regression tests, then source/project lifecycle fixes and narrow native integration API if needed.
3. Parser owner: real AST conversion, services, public helpers/types, ordinary RuleTester fixtures, and focused custom-rule tests. Separate AST support from schema services where practical.
4. Processor owner: extraction/source mapping, virtual-document lifecycle, shim integration, and embedded regression tests. Coordinate contracts with parser/native owners first.
5. Lead: integrate, inspect all diffs, run independent compatibility probes, commission follow-up fixes, and add documentation and performance evidence.

Each owner stages only owned files and uses terse commit messages without conventional prefixes. Tests that reproduce existing incorrect behavior precede fixes. Separate mechanical dependency/formatting work from behavior. Agents report commands, observed output, risks, and commit IDs; the lead verifies them independently.

## Validation and performance gates

- Build the local native addon and plugin. Run existing integration/parity suites before and after implementation.
- Run the same custom rules against upstream v4 and our parser. Compare visitor sequences, raw AST/TypeInfo observations, reports, suggestions, and fixes.
- Exercise inline schema/documents, schema files and extensions, missing schema, physical configs, multiple projects, sibling fragment lookup, and unsaved edits.
- Exercise embedded gql tags and GraphQL comments in JS/TS, multiple blocks, interpolation, Unicode before/inside blocks, CRLF, escaped literals, parser errors, fixes, suggestions, and host-language rules. Include Vue/Svelte/Astro to the extent upstream extraction supports them, and document exact supported forms.
- Compile a consumer fixture using public types and run ordinary RuleTester with our parser on valid and invalid rules.
- Run applicable Rust tests/clippy/formatting, TypeScript builds, lint/format checks, and package dry-run inspection. Investigate every failure; distinguish environmental or pre-existing failures with evidence.
- Benchmark separate processes for fast native rules, compatible native rules, compatible native plus custom rules, and upstream equivalents. Include cold startup, repeated identical inputs, changed inputs, and a schema-aware workload. Use release native builds for published timings, record versions/fixture sizes/iterations and diagnostic equivalence, and report distributions rather than one timing.
- The fast mode must avoid importing compatibility loaders or building JS GraphQL ASTs. Verify module loading as well as runtime timing. Benchmarks must not treat stale diagnostics as a speedup.

## Lead checklist

- [x] Draft implementation plan.
- [x] Sol reviewer pressure-tests the plan.
- [x] Lead validates findings and revises decisions.
- [x] Agents implement in intelligible commits.
- [x] Lead reviews and independently validates all work.
- [x] Document measured performance and remaining compatibility limitations.
- [x] Open and verify [draft PR #1134](https://github.com/trevor-scheer/graphql-analyzer/pull/1134).
- [ ] Inspect CI and resolve relevant failures.

## Review decisions and measured outcomes

The Sol plan review corrected the helper name to `requireGraphQLOperations`, identified native source/config lifetime as a prerequisite, and challenged the performance claims. The lead checked each point against upstream 4.4.0 and the native implementation. The proposed blanket native UTF-16 conversion was rejected: production diagnostic conversion already used UTF-16 columns. Missing host prefixes on the first line of embedded documents were reproduced and fixed separately.

The compatibility frontend eagerly loads configured schema and sibling services so invalid configured schemas still produce parse errors. TypeInfo snapshots remain lazy. Local source snapshots and a bounded schema cache avoid rebuilding unchanged project data; metadata and glob membership invalidate snapshots. Unsupported dynamic dependencies bypass snapshots. This trades additional checking for correctness on repeat lint calls.

The lead's independent release benchmark found a 50-file median of 66 ms for fast mode, 170 ms for compatibility mode, and 91 ms upstream. These results do not support a general compatible-mode speedup claim. See [the reproducible performance evidence](./2026-09-05-eslint-custom-rules-benchmarks.md) for distributions, workload limits, and artifact identity.

Independent integration review found and sent back two defects: failed transformed-schema reloads could lose native state and stop retrying, and processor test fixtures resolved against the test runner's working directory. The native reload is transactional, and fixture paths are module-relative. Additional agent review reproduced unsafe autofixes that formed JavaScript interpolation across edit boundaries; those edits are suppressed.

The final independent Sol review reproduced an ESLint persistent-cache failure caused by missing parser/processor metadata. The lead verified that our parser failed while upstream passed, and the implementation agent added failing tests before fixing all four entry points. The final independent plugin run passed 76 tests, including cache reuse across ESLint instances. The lead also ran 344 affected Rust tests, affected clippy checks, repository formatting/lint/typechecking, and external packed-package checks with strict TypeScript declarations. Packed consumers exercised Node 18 and ESLint 8.40, as well as ESLint 9. Existing repository lint warnings remain outside this change.

Sol also reproduced stale native inventory after a new file is added to an existing schema glob. The lead retained this as a documented pre-existing limitation, not a new custom-frontend blocker. Native dependency refresh covers known files; compatible custom-rule services separately detect glob membership changes. Native inventory discovery, rule-option forwarding, and multi-project support need their own implementation scope.

Release-preparation simulation failed under npm 11.19.0 because regeneration produced five native optional-package entries with missing versions. The lead ran the same script in isolated source copies of this branch and unchanged `bb444fe`; both failed with the same five missing entries. No release-workflow changes were made. The packed runtime dependency audit reported no critical or high findings and three moderate entries through Apollo's `uuid` dependency.

## Integration blocker after draft creation

The lead started from stale local `main` at `bb444fe` without refreshing the remote first. After draft creation, GitHub reported merge conflicts and no CI runs. A fetch and non-mutating `git merge-tree` inspection found 12 conflicting files against live `main` at `0735a0a`, including the native host/diagnostic interfaces, plugin entry points, documentation, and package management. Live `main` uses pnpm, ESLint 10, and newer Rust/TypeScript tooling.

No merge or conflict resolution was applied in that turn. The implementation, review results, and benchmarks remained valid for the recorded baseline, but did not validate a port to live `main`. CI monitoring paused at the `babysit` skill's large/risky-change boundary.

## Approved integration onto current main

The user approved updating the draft to latest `main`. The integration target is `0735a0a`; the lead fetched before starting and will recheck the remote before publishing. A merge commit preserves the published reproduction/implementation history.

The implementation agents own three independent integration areas: native project state and diagnostics; parser, public types, and pnpm dependencies; and processor/rule-shim behavior. The lead owns documentation, benchmarks, conflict review, and independent validation. Current-main capabilities take precedence over stale limitation statements: preserve ESLint options, multi-project routing, native suggestions, validation-name stubs, and all five presets.

Validation uses Rust 1.96, pnpm 10.34.4, TypeScript 7, and ESLint 10. The upstream GraphQL-ESLint 4.4 oracle uses the existing `eslint-v9` alias. Rebuild the release native addon, run the combined integration/parity/custom-rule suites, verify supported packed consumers, run formatting/lint/typechecks and documentation builds, and repeat the benchmark with both ESLint versions recorded. A separate reviewer will inspect the combined changes before the PR update and CI monitoring.

- [x] Resolve conflicts without dropping current-main behavior.
- [x] Review and validate the combined implementation.
- [x] Replace stale performance and migration claims with integrated evidence.
- [x] Prepare the updated draft for push and GitHub CI.

### Integration review and local validation

The merge is `e45bf2b`. A fresh fetch before publication still resolved main to `0735a0a`. Cross-owner review and the lead's validation found three integration defects: physical native component analysis discarded the host language, compatible directives could be suppressed twice, and Vue plucking resolved TypeScript 7 without its required runtime API. Regression commits precede their fixes. Review also caught a public type declaring the removed ESLint 10 `context.parserServices` property; helpers and types now use `context.sourceCode.parserServices` on every supported version.

Native calls retain project routing, per-call rule options, suggestions, and independent source overlays. Compatible directives delegate to ESLint through a Salsa-tracked per-call flag; native analyzer ignores remain active, and the flag is restored afterward. Fast-mode suppressions accept canonical and recognized GraphQL-ESLint rule names. Normal embedded documents still share one physical native analysis. Existing native glob-discovery and extraction limits remain documented.

The lead independently ran the 98-test plugin suite both in the worktree and with a fresh frozen pnpm install, plus 1,258 affected Rust tests, repository formatting/lint/typechecks, and the 72-page documentation build. A throwaway checkout passed the release-prep simulation. Final packed consumers passed runtime and strict declaration checks on ESLint 8.40/TypeScript 5, ESLint 9/TypeScript 6, and ESLint 10/TypeScript 7. The first two also passed Vue/Svelte extraction on Node 18. Existing lint warnings remain. The benchmark's alias-version reporting failure received a separate regression test and fix before rerunning measurements.

The renewed release benchmark uses clean revision `8b0bfbe`, holds ESLint 9 constant for the primary comparison, and records a separate analyzer-on-ESLint-10 run. The [integrated evidence](./2026-09-05-eslint-custom-rules-benchmarks.md) retains the compatibility-cost caveat. The earlier Sol plan review remains applicable; the merged implementation also received cross-owner review and independent lead validation. GitHub check status is tracked on draft PR #1134 after this local checkpoint.
