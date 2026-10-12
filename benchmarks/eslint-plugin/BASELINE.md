# ESLint plugin benchmark baseline

The October 12, 2026 UTC release-mode run found **zero diagnostic differences** across the measured upstream, compatible, fast native, and custom-rule lanes. The existing per-rule parity tests also passed. This is workload parity, not full feature parity: the analyzer still exports 30 validation-rule stubs that do not emit rule-specific diagnostics.

## Inputs and environment

- Machine: Apple M3, darwin arm64; Node v24.15.0; ESLint 9.39.4; `@graphql-eslint/eslint-plugin` 4.4.1; GraphQL 16.13.2.
- Analyzer plugin 0.1.4, core 0.1.3; native build: release; addon SHA-256: `f266bd1289af669a723b58bcdb37b500dbd5a1716cd732f9cbb202e6838d0098`.
- 3 fresh-process samples, 1 warmup, 3 repeated warm/project measurements per sample; 12 generated operations per schema.
- Coverage: 34 active shared rules, 30 validation stubs, 4 analyzer-only rules, 0 missing upstream names.

The schema snapshots and their source URLs are pinned in [`manifest.json`](manifest.json).

| Schema  |     Bytes | Types | Fields | SHA-256                                                            |
| ------- | --------: | ----: | -----: | ------------------------------------------------------------------ |
| github  | 1,562,177 | 1,829 |  8,420 | `3ed095654b22b1a8e7a88af36af2b28cb6d9613da332e9c57303e30476958d42` |
| graphos |   255,835 |   524 |  1,594 | `02559ddbe70e22db15a761cf96c363dc0e6f76f3a540eb10f59243ed56133e8f` |
| saleor  | 1,065,360 | 1,470 |  6,374 | `85f0363e446d2ce59ed3fa1a2a8c4f00406f481cbbb8f97bfa01fb2a5cc6ca9c` |

## Measurements

Times are milliseconds. Each cell shows the median followed by p95 in parentheses. RSS is the median observed process snapshot in MiB; it is not peak memory. The same ESLint 9 runtime and input files were used for each comparable lane.

| Schema  | Workload   | Lane              |    Cold lint ms |    Warm lint ms | Observed RSS MiB |
| ------- | ---------- | ----------------- | --------------: | --------------: | ---------------: |
| github  | operations | upstream          |  347.97 (349.7) |    7.95 (13.42) |           342.88 |
| github  | operations | compatible        | 391.88 (391.97) |   10.56 (11.12) |           299.38 |
| github  | operations | fast              |   78.43 (78.48) |      2.7 (4.34) |           102.56 |
| github  | operations | upstream-custom   | 349.63 (353.66) |    8.38 (13.73) |           344.45 |
| github  | operations | compatible-custom | 394.31 (395.18) |   11.05 (12.73) |           298.23 |
| github  | schema     | upstream          | 540.61 (549.93) | 209.65 (287.25) |          1782.88 |
| github  | schema     | compatible        | 625.49 (644.62) | 167.25 (207.82) |          1284.31 |
| github  | schema     | fast              | 138.79 (140.48) |   24.16 (25.66) |           277.28 |
| github  | embedded   | upstream          | 337.89 (345.61) |     1.67 (2.02) |           318.56 |
| github  | embedded   | compatible        | 378.01 (378.22) |     4.05 (8.26) |           295.33 |
| github  | embedded   | upstream-custom   | 343.13 (343.26) |     1.92 (6.53) |           320.16 |
| github  | embedded   | compatible-custom | 384.62 (385.13) |     5.26 (7.08) |           300.55 |
| graphos | operations | upstream          | 102.88 (104.44) |     6.93 (9.36) |           188.41 |
| graphos | operations | compatible        | 220.31 (221.02) |   10.05 (11.66) |           182.67 |
| graphos | operations | fast              |   26.57 (27.31) |      2.6 (2.93) |            89.64 |
| graphos | operations | upstream-custom   |  100.6 (101.67) |    7.33 (10.59) |           188.84 |
| graphos | operations | compatible-custom | 221.03 (222.54) |   10.53 (11.28) |           183.88 |
| graphos | schema     | upstream          | 130.77 (132.39) |   35.28 (41.18) |           448.05 |
| graphos | schema     | compatible        | 253.77 (389.28) |   31.56 (37.76) |           478.59 |
| graphos | schema     | fast              |    30.5 (30.84) |     3.73 (6.01) |           109.72 |
| graphos | embedded   | upstream          |  97.05 (100.55) |      1.75 (1.9) |           182.42 |
| graphos | embedded   | compatible        |  208.6 (213.32) |     2.87 (3.46) |           175.36 |
| graphos | embedded   | upstream-custom   |    97.74 (99.3) |     1.78 (1.87) |           182.55 |
| graphos | embedded   | compatible-custom | 211.83 (215.18) |     3.06 (3.66) |           175.83 |
| saleor  | operations | upstream          | 287.97 (290.89) |     7.87 (8.47) |           296.86 |
| saleor  | operations | compatible        | 350.07 (350.09) |   10.46 (16.69) |           286.02 |
| saleor  | operations | fast              |   64.72 (65.28) |     2.84 (4.19) |           100.34 |
| saleor  | operations | upstream-custom   | 286.15 (299.05) |     8.07 (8.68) |           299.67 |
| saleor  | operations | compatible-custom | 357.72 (361.59) |   11.57 (18.29) |           285.97 |
| saleor  | schema     | upstream          | 472.11 (477.25) | 186.17 (268.93) |          1453.83 |
| saleor  | schema     | compatible        | 558.16 (558.92) | 163.87 (260.78) |          1536.86 |
| saleor  | schema     | fast              | 105.54 (105.67) |   15.47 (19.39) |           224.97 |
| saleor  | embedded   | upstream          | 259.46 (263.31) |     2.85 (6.22) |           299.39 |
| saleor  | embedded   | compatible        | 335.62 (335.64) |      4.02 (7.3) |           281.86 |
| saleor  | embedded   | upstream-custom   |  260.93 (265.8) |     2.28 (7.09) |           297.78 |
| saleor  | embedded   | compatible-custom |  336.69 (336.9) |     4.96 (7.75) |           284.59 |

## Interpretation and limits

- The fast native lane had the lowest median cold and warm lint time in these local workloads. Compatible mode paid for JavaScript GraphQL AST conversion and was often slower than upstream. These observations do not establish a general speedup for the compatible parser.
- Direct schema linting included the full public SDL plus one comment violation. Operation linting used 12 deterministic, validated documents with one duplicate field each. Embedded linting used one JavaScript `gql` template and compared the same custom `typeInfo()` rule.
- The 30 validation-rule stubs accept community configuration names but do not expose the matching rule-specific reports. Built-in validation is separate. The suite checks active rule behavior and reports this gap explicitly.
- Three process samples are enough to expose large differences but too few for a statistical performance guarantee. Processes ran sequentially on a shared development machine. The observed RSS snapshots can miss transient peaks.
- GitHub and GraphOS publish mutable schemas. A fresh checkout may fail the pinned hash check after either source changes. Exact reproduction of this baseline then requires its locally cached snapshots; a reviewed manifest refresh measures a new corpus. Saleor uses an immutable source commit.
- The report recorded `dirty: true`: the run used PR #1134 revision `a717e3a29dc41639286f66e65c24f117676a2125` with the new suite uncommitted. Reproduce the run on the committed branch to capture its final Git revision.

Run `BENCH_NATIVE_BUILD=release node benchmarks/eslint-plugin/run.mjs bench --samples=3 --repeats=3 --warmups=1` after following the setup steps in [`README.md`](README.md). The command writes detailed local JSON and Markdown reports under the ignored `reports/` directory.
