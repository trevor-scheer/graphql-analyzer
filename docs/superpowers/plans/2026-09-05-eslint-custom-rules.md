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
- [ ] Lead reviews and independently validates all work.
- [x] Document measured performance and remaining compatibility limitations.
- [ ] Open and verify a draft PR; inspect CI and resolve relevant failures.

## Review decisions and measured outcomes

The Sol plan review corrected the helper name to `requireGraphQLOperations`, identified native source/config lifetime as a prerequisite, and challenged the performance claims. The lead checked each point against upstream 4.4.0 and the native implementation. The proposed blanket native UTF-16 conversion was rejected: production diagnostic conversion already used UTF-16 columns. Missing host prefixes on the first line of embedded documents were reproduced and fixed separately.

The compatibility frontend eagerly loads configured schema and sibling services so invalid configured schemas still produce parse errors. TypeInfo snapshots remain lazy. Local source snapshots and a bounded schema cache avoid rebuilding unchanged project data; metadata and glob membership invalidate snapshots. Unsupported dynamic dependencies bypass snapshots. This trades additional checking for correctness on repeat lint calls.

The lead's independent release benchmark found a 50-file median of 66 ms for fast mode, 170 ms for compatibility mode, and 91 ms upstream. These results do not support a general compatible-mode speedup claim. See [the reproducible performance evidence](./2026-09-05-eslint-custom-rules-benchmarks.md) for distributions, workload limits, and artifact identity.

Independent integration review found and sent back two defects: failed transformed-schema reloads could lose native state and stop retrying, and processor test fixtures resolved against the test runner's working directory. The native reload is transactional, and fixture paths are module-relative. Additional agent review reproduced unsafe autofixes that formed JavaScript interpolation across edit boundaries; those edits are suppressed.
