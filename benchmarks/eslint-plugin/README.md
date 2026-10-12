# ESLint plugin parity and performance suite

This suite compares `@graphql-analyzer/eslint-plugin` with the community
`@graphql-eslint/eslint-plugin` on the same GraphQL files and ESLint 9 runtime.
It tests the public GitHub API schema, Apollo GraphOS Platform API schema, and
a Saleor schema pinned to a source commit. All corpus preparation happens before
the timed runs. Compressed, pinned schema snapshots are tracked under
`snapshots/` so CI and fresh checkouts use the same inputs. Expanded schemas,
generated operations, and reports are ignored by Git.

## Setup

From the repository root:

```sh
pnpm install --frozen-lockfile
pnpm --filter @graphql-analyzer/core run build
pnpm --filter @graphql-analyzer/eslint-plugin run build
node benchmarks/eslint-plugin/fetch.mjs
BENCH_FILES=12 node benchmarks/eslint-plugin/generate.mjs
```

`fetch.mjs` verifies cached files against `manifest.json`. If the cache is
empty, it expands the tracked snapshots and checks their pinned SHA-256. The
timed run does not fetch schemas. A reviewed manifest and snapshot refresh
measures a new corpus.
To review and deliberately update a source snapshot, run
`node benchmarks/eslint-plugin/fetch.mjs --refresh`, inspect the changed
`manifest.json` and compressed snapshots, then regenerate. Saleor's URL uses a
fixed commit. The manifest records retrieval date, URL, SHA-256, bytes, type count, and field
count. `generate.mjs` checks the hashes, creates deterministic operations
offline, and validates each operation against its schema. Set `BENCH_FILES`
to a positive integer to change the project size.

## Parity

```sh
node benchmarks/eslint-plugin/run.mjs parity
```

This runs the existing rule-by-rule parity tests, then compares diagnostics
on each large corpus. It covers operation files, direct linting of the full
schema, and a JavaScript file with embedded GraphQL. It fails on parser errors,
missing upstream rule names, or differences in active rule diagnostics. The
comparison includes rule, severity, message, location, fixes, and suggestions.
Each workload has a deliberate rule violation so the comparison cannot pass
with empty diagnostics. An edited, valid version must change the result.

The report lists active shared rules, validation-rule stubs, and rules unique
to either plugin. Validation stubs accept community rule names in configs but
do not report those rule-specific diagnostics. Native validation runs
separately, so the suite reports this as a parity gap. The existing small
fixtures remain the authoritative per-rule option and suggestion checks;
the large corpora add scale and schema diversity.

For a shorter development pass:

```sh
node benchmarks/eslint-plugin/run.mjs parity --corpus=graphos --scenario=embedded --samples=1 --repeats=1 --skip-rule-tests
```

## Performance

```sh
node benchmarks/eslint-plugin/run.mjs bench --samples=5 --repeats=5 --warmups=2
```

Each lane runs in a fresh process. Operation lanes are community upstream,
analyzer compatible parser, analyzer fast native parser, and upstream/compatible
with the same custom `typeInfo()` rule. Direct schema linting uses a schema
rule across upstream, compatible, and fast parsers. Embedded JavaScript uses
the documented host processor plus virtual GraphQL parser configuration and
compares upstream and compatible modes, with and without that custom rule.
The suite rotates lane order between samples, measures module loading, first
result, cold lint, warm lint, and alternating changed-file lint, and records
median, p95, range, and observed RSS. It checks diagnostic equivalence in
performance runs too. There is no timing pass/fail threshold; noisy shared CI
machines are not a reliable basis for one.

Reports are written to `benchmarks/eslint-plugin/reports/` as JSON and
Markdown. They identify the Git revision and dirty state, Node/ESLint/GraphQL
versions, CPU, platform, native addon hash, schema hashes, and workload size.
The `ESLint benchmark` GitHub Actions workflow runs the three corpora on
relevant pull requests and by manual dispatch. Its job summary compares cold,
warm, and changed-file medians with the community plugin. The JSON and Markdown
reports are attached to the run for 30 days. Timings are observations, not a CI
pass/fail threshold; diagnostic mismatches still fail the job.
The native build command above uses release mode. Compare runs only when
machine, dependency versions, build mode, source hashes, file count, and rule
set match. These workloads cover local SDL, GraphQL documents, and one embedded
JavaScript template. They do not measure remote schema loading or every host
language supported by the plugin.
