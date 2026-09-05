# Custom-rule compatibility performance evidence

The compatible parser preserves custom-rule behavior at a measurable cost in this fixture. Its repeated lint and 50-file batch runs were slower than GraphQL-ESLint 4.4.0. The explicit fast parser avoided compatibility dependencies and retained lower startup and batch times. These measurements do not support a general speedup claim for the compatible parser.

## Reproduce the comparison

Build the native addon in release mode and compile the plugin before running the benchmark from the repository root:

```sh
npm run build --workspace=@graphql-analyzer/core
npm run build --workspace=@graphql-analyzer/eslint-plugin
BENCH_NATIVE_BUILD=release node packages/eslint-plugin/scripts/benchmark.mjs
```

The script writes JSON to stdout. `BENCH_NATIVE_BUILD` records the caller's build label; it does not detect the Rust optimization level. The output includes the native artifact's SHA-256 hash, revision, dirty-worktree status, dependency versions, fixture sizes, and timing distributions.

The defaults are 50 GraphQL operation files, three fresh processes per comparison, and three repeated calls for each single-file scenario. Each operation has 41 field selections. The schema has 100 fields. Increase or decrease the workload with `BENCH_FILES`, `BENCH_SAMPLES`, and `BENCH_REPEATS`:

```sh
BENCH_NATIVE_BUILD=release BENCH_FILES=100 BENCH_SAMPLES=5 BENCH_REPEATS=10 node packages/eslint-plugin/scripts/benchmark.mjs
```

Each child process has a 60-second timeout. The script creates a temporary project and removes it after the run.

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

The lead's independent measurements used Node.js 24.20.0, ESLint 9.39.3, GraphQL-ESLint 4.4.0, graphql-js 16.12.0, Linux x64, and an AMD Ryzen 7 5700U. The release artifact was `graphql-analyzer.linux-x64-gnu.node`, SHA-256 `8cf0221889b9f192e20104fc7420107f8251bd833022c15a6c24bda77aa65386`.

The recorded HEAD was `2f11865bdcabfeeea2cc005dee3b6dba4d189638`. All measured production sources and the benchmark script were committed. The worktree was marked dirty because test-fixture and documentation work was still in progress. The run completed at `2026-09-05T16:39:47Z`.

Times below are milliseconds, shown as median with minimum–maximum in parentheses. The repeated and changed columns each combine nine calls across three processes. The other columns each contain three observations.

| Configuration                                            |           First result |  Repeated source |   Changed source |  50-file project batch |
| -------------------------------------------------------- | ---------------------: | ---------------: | ---------------: | ---------------------: |
| Fast parser, native rules                                | 142.55 (139.48–143.91) | 0.80 (0.65–0.92) | 1.11 (0.90–1.19) |    65.64 (62.39–70.78) |
| Compatible parser, native rules                          | 618.09 (609.38–622.98) | 4.17 (3.81–4.50) | 4.18 (3.93–4.40) | 169.89 (163.54–172.49) |
| Compatible parser, native rules and custom TypeInfo rule | 634.14 (626.82–665.41) | 4.65 (4.14–5.60) | 4.47 (4.33–5.36) | 179.54 (177.45–204.77) |
| Upstream parser and rules                                | 665.05 (644.62–680.20) | 2.74 (2.26–3.07) | 2.67 (2.29–3.11) |    91.42 (89.16–91.81) |
| Upstream parser, rules, and custom TypeInfo rule         | 627.82 (615.45–628.41) | 2.37 (2.10–2.79) | 2.56 (2.30–3.02) |    87.52 (82.18–98.67) |

Memory readings were taken after the project batch, without forcing garbage collection. Median RSS was 88 MiB for fast mode, 181 MiB for compatible native rules, 184 MiB for compatible native plus custom rules, 170 MiB for upstream built-ins, and 169 MiB for upstream plus custom rules. These readings are process snapshots, not peak-memory measurements.

## Interpretation and limits

The compatible parser's first result was in the same broad range as upstream. Its repeated-source median was about 1.5 times upstream's, and its project batch was about 1.9 times upstream's. Adding the TypeInfo custom rule did not explain the difference in this fixture.

An earlier implementation loaded schema and sibling sources on every parse. That provisional run measured a 670 ms compatible batch. The implementation now caches unchanged local GraphQL/JSON sources and built schemas, checks dependency metadata, and expands globs to detect added or deleted files. Executable sources, custom loaders, remote pointers, dynamic loader options, and schema imports bypass source snapshots. The before/after runs also included other native changes, so the difference is not an isolated estimate of the cache's benefit.

Compatibility mode builds a JavaScript GraphQL AST and runs ESLint traversal in addition to native analysis. Native rules still have their individual traversals. Dependency refresh also scans metadata for known native files. This run does not isolate those costs from config loading, AST conversion, or garbage collection.

The fast comparison retained its small JavaScript AST and passed the dependency-loading assertion. Its results apply to these two native rules and this local fixture. They do not establish performance for every built-in rule, remote schemas, large schemas, embedded documents, or editor sessions. They also do not show unchanged performance against the pre-PR implementation: correctness fixes add dependency checks, and the old stale-source behavior is not a valid performance baseline.

Processes ran sequentially in a shared development environment. Three samples reveal basic variation but are too few for statistical performance guarantees. The script has no speed threshold; diagnostic equivalence and fast-mode dependency isolation are its correctness gates.
