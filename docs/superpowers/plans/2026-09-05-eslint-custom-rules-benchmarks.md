# Custom-rule compatibility performance evidence

The compatible parser preserves custom-rule behavior at a measurable cost in this fixture. Its repeated lint and 50-file batch runs were slower than GraphQL-ESLint 4.4.0. The explicit fast parser avoided compatibility dependencies and retained lower startup and batch times. These measurements do not support a general speedup claim for the compatible parser.

## Reproduce the comparison

Build the native addon in release mode and compile the plugin before running the benchmark from the repository root:

```sh
pnpm install --frozen-lockfile
pnpm --filter @graphql-analyzer/core run build
pnpm --filter @graphql-analyzer/eslint-plugin run build
BENCH_NATIVE_BUILD=release node packages/eslint-plugin/scripts/benchmark.mjs
```

The script writes JSON to stdout. `BENCH_NATIVE_BUILD` records the caller's build label; it does not detect the Rust optimization level. The output includes the native artifact's SHA-256 hash, revision, dirty-worktree status, dependency versions, fixture sizes, and timing distributions.

The defaults are 50 GraphQL operation files, three fresh processes per comparison, and three repeated calls for each single-file scenario. Each operation has 41 field selections. The schema has 100 fields. Increase or decrease the workload with `BENCH_FILES`, `BENCH_SAMPLES`, and `BENCH_REPEATS`:

```sh
BENCH_NATIVE_BUILD=release BENCH_FILES=100 BENCH_SAMPLES=5 BENCH_REPEATS=10 node packages/eslint-plugin/scripts/benchmark.mjs
```

Each child process has a 60-second timeout. The script creates a temporary project and removes it after the run.

The default comparison uses the `eslint-v9` alias for every configuration, so the ESLint version is held constant. To also measure the analyzer on main's ESLint 10 dependency, run:

```sh
BENCH_NATIVE_BUILD=release BENCH_ANALYZER_ESLINT=eslint node packages/eslint-plugin/scripts/benchmark.mjs
```

Upstream stays on its supported ESLint 9 version in that second run. Its cross-version timings do not isolate the plugin's cost. Both ESLint versions are recorded in the output.

## Workloads and correctness checks

All five comparisons enable `no-anonymous-operations` and `no-duplicate-fields` against the same schema and documents. The two custom-rule comparisons add the same JavaScript rule. It calls `typeInfo()` for every field, asserts that the schema type exists, and reports one field per operation.

The script checks the full built-in diagnostic shape across all comparisons, including message text, message ID, positions, and severity. It also checks custom-rule diagnostics against upstream. The initial source produces one built-in diagnostic. Replacing a duplicate field with a different field of the same string length removes it. Alternating those inputs verifies that changed-input timing does not reward stale diagnostics. The 50-file batch produces 50 built-in diagnostics, plus 50 custom diagnostics where enabled.

The script measures these distinct costs:

- Module loading includes ESLint and the selected plugin. Lazy compatibility imports occur during the first lint instead.
- First result includes module loading, linter construction, and the first single-file lint. It makes the startup comparison meaningful despite different import timing.
- Repeated source measures identical single-file input after the first lint.
- Changed source alternates between two inputs of the same length.
- Project batch runs `lintFiles` on all 50 files after the single-file calls. It uses a warmed process, schema, and project inventory; it is not a cold project run.

The fast comparison also asserts that the Node module cache contains no `graphql`, `graphql-config`, or `@graphql-tools/graphql-tag-pluck` modules after linting.

## September 5, 2026 measurements

The lead's independent measurements used Node.js 24.20.0, ESLint 9.39.4, GraphQL-ESLint 4.4.0, graphql-js 16.13.2, Linux x64, and an AMD Ryzen 7 5700U. The release artifact was `graphql-analyzer.linux-x64-gnu.node`, SHA-256 `f845dbadd2a33e9faed4ecdc88b966b673d7688cd596416e616d550198b88182`.

The recorded HEAD was `8b0bfbe085ce9e8b52bb6ad7ec5866ff5fad8068`, which includes main at `0735a0a`. The worktree was clean. The same-version run completed at `2026-09-05T20:51:57Z`. These results replace the earlier stale-base measurements; they are not a controlled before/after comparison with that implementation.

Times below are milliseconds, shown as median with minimum–maximum in parentheses. The repeated and changed columns each combine nine calls across three processes. The other columns each contain three observations.

| Configuration                                            |           First result |  Repeated source |   Changed source |  50-file project batch |
| -------------------------------------------------------- | ---------------------: | ---------------: | ---------------: | ---------------------: |
| Fast parser, native rules                                | 134.23 (129.04–135.44) | 1.04 (0.92–1.15) | 1.14 (0.90–1.28) |    68.03 (65.22–70.03) |
| Compatible parser, native rules                          | 430.47 (427.62–443.90) | 3.72 (3.57–4.12) | 3.85 (3.49–4.30) | 177.55 (176.69–183.28) |
| Compatible parser, native rules and custom TypeInfo rule | 435.99 (432.94–447.63) | 4.11 (3.76–4.43) | 4.36 (4.15–4.49) | 180.39 (179.27–182.98) |
| Upstream parser and rules                                | 467.59 (458.44–471.79) | 2.23 (1.97–2.63) | 2.24 (2.05–2.51) |    77.17 (76.33–82.12) |
| Upstream parser, rules, and custom TypeInfo rule         | 443.67 (433.65–445.27) | 2.29 (1.92–2.68) | 2.32 (2.06–3.10) |    84.48 (79.51–85.61) |

Memory readings were taken after the project batch, without forcing garbage collection. Median RSS was 87 MiB for fast mode, 176 MiB for compatible native rules, 183 MiB for compatible native plus custom rules, 157 MiB for upstream built-ins, and 157 MiB for upstream plus custom rules. These readings are process snapshots, not peak-memory measurements.

### Analyzer on ESLint 10

The second run used ESLint 10.6.0 for analyzer configurations and ESLint 9.39.4 for upstream. It used the same clean revision, native artifact, fixture, and sample counts, and completed at `2026-09-05T20:52:06Z`. Both runs passed diagnostic-equivalence and fast-mode dependency checks.

| Configuration                                               |           First result |  Repeated source |   Changed source |  50-file project batch |
| ----------------------------------------------------------- | ---------------------: | ---------------: | ---------------: | ---------------------: |
| Fast parser, native rules                                   | 123.54 (121.74–126.60) | 1.26 (1.12–1.45) | 1.29 (1.19–5.65) |    80.39 (69.20–82.37) |
| Compatible parser, native rules                             | 425.85 (421.71–432.41) | 3.80 (3.39–4.14) | 3.73 (3.29–4.18) | 177.40 (163.98–181.83) |
| Compatible parser, native rules and custom TypeInfo rule    | 425.80 (419.14–434.54) | 4.13 (3.71–4.43) | 3.94 (3.77–4.77) | 187.21 (173.82–196.26) |
| Upstream parser and rules (ESLint 9)                        | 448.99 (443.30–449.07) | 2.15 (1.94–2.52) | 2.33 (1.98–2.83) |    86.29 (84.97–88.64) |
| Upstream parser, rules, and custom TypeInfo rule (ESLint 9) | 466.67 (449.36–494.13) | 2.21 (1.87–2.62) | 2.43 (2.03–3.14) |    78.64 (76.44–93.74) |

## Interpretation and limits

In the same-version comparison, the compatible parser's first result was in the same broad range as upstream. Its repeated-source median was about 1.7 times upstream's, and its project batch was about 2.3 times upstream's. Adding the TypeInfo custom rule did not explain the difference in this fixture.

The implementation caches unchanged local GraphQL/JSON sources and built schemas, checks dependency metadata, and expands custom-service globs to detect added or deleted files. Executable sources, custom loaders, remote pointers, dynamic loader options, and schema imports bypass source snapshots. These runs do not isolate the cache's benefit.

Compatibility mode builds a JavaScript GraphQL AST and runs ESLint traversal in addition to native analysis. Native rules still have their individual traversals. Dependency refresh also scans metadata for known native files. This run does not isolate those costs from config loading, AST conversion, or garbage collection.

The fast comparison retained its small JavaScript AST and passed the dependency-loading assertion. Its results apply to these two native rules and this local fixture. They do not establish performance for every built-in rule, remote schemas, large schemas, embedded documents, or editor sessions. They also do not show unchanged performance against the pre-PR implementation: correctness fixes add dependency checks, and the old stale-source behavior is not a valid performance baseline.

Processes ran sequentially in a shared development environment. Three samples reveal basic variation but are too few for statistical performance guarantees. The script has no speed threshold; diagnostic equivalence and fast-mode dependency isolation are its correctness gates.
