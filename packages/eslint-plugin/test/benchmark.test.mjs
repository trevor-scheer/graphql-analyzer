import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);

test("benchmark reports aliased ESLint versions after checking diagnostic equivalence", () => {
  const result = spawnSync(
    process.execPath,
    [fileURLToPath(new URL("../scripts/benchmark.mjs", import.meta.url))],
    {
      encoding: "utf8",
      timeout: 30_000,
      env: {
        ...process.env,
        BENCH_FILES: "1",
        BENCH_SAMPLES: "1",
        BENCH_REPEATS: "1",
        BENCH_ANALYZER_ESLINT: "eslint-v9",
      },
    },
  );
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  const report = JSON.parse(result.stdout);
  const eslintVersion = require("eslint-v9/package.json").version;
  assert.equal(report.metadata.eslint, eslintVersion);
  assert.equal(report.metadata.upstreamEslint, eslintVersion);
  assert.deepEqual(report.equivalence, {
    nativeDiagnosticsPerColdLint: 1,
    nativeDiagnosticsChangedLint: 0,
    nativeDiagnosticsProject: 1,
    customDiagnosticsPerColdLint: 1,
    customDiagnosticsProject: 1,
  });
  assert.equal(report.results.fast.loadedCompatibility, false);
  assert.deepEqual(Object.keys(report.results), [
    "fast",
    "compatible",
    "compatible-custom",
    "upstream",
    "upstream-custom",
  ]);
});
