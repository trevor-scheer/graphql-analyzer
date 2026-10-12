import assert from "node:assert/strict";
import { spawnSync, execFileSync } from "node:child_process";
import { cpus, arch, platform } from "node:os";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  directory,
  json,
  pluginDirectory,
  projects,
  reportDirectory,
  requirePlugin,
  schemas,
  sha256,
  summary,
  root,
} from "./shared.mjs";

const mode = process.argv[2];
if (!["parity", "bench"].includes(mode)) {
  throw new Error(
    "Usage: node benchmarks/eslint-plugin/run.mjs parity|bench [--corpus=github|graphos|saleor] [--scenario=operations|schema|embedded] [--samples=N] [--repeats=N] [--warmups=N] [--skip-rule-tests]",
  );
}
function argument(name, fallback) {
  return process.argv.find((entry) => entry.startsWith(`--${name}=`))?.split("=")[1] ?? fallback;
}
function positive(name, fallback) {
  const value = Number(argument(name, fallback));
  if (!Number.isInteger(value) || value < 1) throw new Error(`${name} must be a positive integer`);
  return value;
}
const samples = positive("samples", mode === "parity" ? 1 : 3);
const repeats = positive("repeats", mode === "parity" ? 1 : 3);
const warmups = positive("warmups", 1);
const onlyCorpus = argument("corpus", undefined);
const onlyScenario = argument("scenario", undefined);
const manifestPath = join(directory, "manifest.json");
if (!existsSync(manifestPath))
  throw new Error("No schema manifest. Run fetch.mjs then generate.mjs.");
const manifest = json(manifestPath);
const corpora = onlyCorpus ? [onlyCorpus] : Object.keys(manifest);
for (const name of corpora) {
  if (!manifest[name]) throw new Error(`Unknown corpus: ${name}`);
  const schema = join(schemas, `${name}.graphql`);
  const projectSchema = join(projects, name, "schema.graphql");
  if (!existsSync(schema) || !existsSync(projectSchema)) {
    throw new Error(`${name}: missing cache/project. Run fetch.mjs and generate.mjs.`);
  }
  if (
    sha256(readFileSync(schema)) !== manifest[name].sha256 ||
    sha256(readFileSync(projectSchema)) !== manifest[name].sha256
  ) {
    throw new Error(
      `${name}: schema hash differs from pinned manifest. Regenerate from verified cache.`,
    );
  }
}

const scenarioLanes = {
  operations: ["upstream", "compatible", "fast", "upstream-custom", "compatible-custom"],
  schema: ["upstream", "compatible", "fast"],
  embedded: ["upstream", "compatible", "upstream-custom", "compatible-custom"],
};
if (onlyScenario && !scenarioLanes[onlyScenario])
  throw new Error(`Unknown scenario: ${onlyScenario}`);
const scenarios = onlyScenario ? [onlyScenario] : Object.keys(scenarioLanes);
const worker = fileURLToPath(new URL("./worker.mjs", import.meta.url));
const oursModule = requirePlugin("./dist/index.js");
const ours = oursModule.default ?? oursModule;
const theirsModule = requirePlugin("@graphql-eslint/eslint-plugin");
const theirs = theirsModule.default ?? theirsModule;
const theirsRules = new Set(Object.keys(theirs.rules));
const oursRules = new Set(Object.keys(ours.rules));
const stubRules = [...oursRules]
  .filter((name) =>
    ours.rules[name].meta?.docs?.description?.includes(
      "This rule does not report validation diagnostics.",
    ),
  )
  .sort();
const coverage = {
  upstreamRules: theirsRules.size,
  analyzerRules: oursRules.size,
  activeShared: [...oursRules]
    .filter((name) => theirsRules.has(name) && !stubRules.includes(name))
    .sort(),
  validationStubs: stubRules,
  analyzerOnly: [...oursRules].filter((name) => !theirsRules.has(name)).sort(),
  upstreamOnly: [...theirsRules].filter((name) => !oursRules.has(name)).sort(),
};

let ruleTests = { status: "not run" };
if (mode === "parity" && !process.argv.includes("--skip-rule-tests")) {
  const result = spawnSync(process.execPath, ["--test", "test/parity.test.mjs"], {
    cwd: pluginDirectory,
    encoding: "utf8",
    env: { ...process.env, NODE_ENV: "test" },
    timeout: 300_000,
    maxBuffer: 16 * 1024 * 1024,
  });
  ruleTests = {
    status: result.status === 0 ? "passed" : "failed",
    output: result.stdout.slice(-5000),
    error: result.stderr.slice(-2000),
  };
  console.log(`Existing per-rule parity tests: ${ruleTests.status}`);
}

function packageVersion(name) {
  return requirePlugin(`${name}/package.json`).version;
}
const nativeArtifacts = readdirSync(join(root, "packages/core"))
  .filter((filename) => filename.endsWith(".node"))
  .map((filename) => ({
    filename,
    sha256: sha256(readFileSync(join(root, "packages/core", filename))),
  }));
if (!nativeArtifacts.length)
  throw new Error("No native addon found. Build @graphql-analyzer/core in release mode first.");
const observations = Object.fromEntries(
  corpora.map((name) => [
    name,
    Object.fromEntries(
      scenarios.map((scenario) => [
        scenario,
        Object.fromEntries(scenarioLanes[scenario].map((lane) => [lane, []])),
      ]),
    ),
  ]),
);
const mismatches = [];

for (const name of corpora) {
  const project = join(projects, name);
  for (const scenario of scenarios) {
    const lanes = scenarioLanes[scenario];
    for (let sample = 0; sample < samples; sample++) {
      const order = [
        ...lanes.slice(sample % lanes.length),
        ...lanes.slice(0, sample % lanes.length),
      ];
      for (const lane of order) {
        const result = spawnSync(
          process.execPath,
          [worker, lane, project, String(repeats), String(warmups), scenario],
          {
            cwd: root,
            encoding: "utf8",
            env: { ...process.env, NODE_ENV: "test" },
            timeout: 300_000,
            maxBuffer: 64 * 1024 * 1024,
          },
        );
        if (result.status !== 0) {
          throw new Error(
            `${name}/${scenario}/${lane}/sample ${sample + 1} failed:\n${result.stderr || result.error?.message}\n${result.stdout}`,
          );
        }
        observations[name][scenario][lane].push(JSON.parse(result.stdout));
        console.log(`${name} ${scenario} ${lane} sample ${sample + 1}/${samples}`);
      }
      const base = observations[name][scenario].upstream[sample];
      assert.ok(
        base.diagnostics.project.length > 0,
        `${name}/${scenario}: fixture produced no native diagnostics`,
      );
      for (const lane of lanes.filter((candidate) => candidate !== "upstream")) {
        const observed = observations[name][scenario][lane][sample];
        for (const key of ["project", "valid"]) {
          if (JSON.stringify(observed.diagnostics[key]) !== JSON.stringify(base.diagnostics[key])) {
            mismatches.push({
              corpus: name,
              scenario,
              sample,
              lane,
              kind: key,
              expected: base.diagnostics[key],
              actual: observed.diagnostics[key],
            });
          }
        }
      }
      if (lanes.includes("upstream-custom")) {
        const customExpected =
          observations[name][scenario]["upstream-custom"][sample].diagnostics.custom;
        const customActual =
          observations[name][scenario]["compatible-custom"][sample].diagnostics.custom;
        if (!customExpected.length)
          mismatches.push({ corpus: name, scenario, sample, kind: "custom-empty" });
        if (JSON.stringify(customActual) !== JSON.stringify(customExpected)) {
          mismatches.push({
            corpus: name,
            scenario,
            sample,
            lane: "compatible-custom",
            kind: "custom",
            expected: customExpected,
            actual: customActual,
          });
        }
      }
    }
  }
}

const results = Object.fromEntries(
  corpora.map((name) => [
    name,
    Object.fromEntries(
      scenarios.map((scenario) => [
        scenario,
        Object.fromEntries(
          scenarioLanes[scenario].map((lane) => {
            const runs = observations[name][scenario][lane];
            return [
              lane,
              {
                moduleLoadMs: summary(runs.map((run) => run.timings.moduleLoadMs)),
                firstResultMs: summary(runs.map((run) => run.timings.firstResultMs)),
                coldProjectMs: summary(runs.map((run) => run.timings.coldProjectMs)),
                warmProjectMs: summary(runs.flatMap((run) => run.timings.warmProjectMs)),
                changedLintMs: summary(runs.flatMap((run) => run.timings.changedLintMs)),
                rssMiB: summary(runs.map((run) => run.rssMiB)),
                diagnostics: runs[0].diagnostics.project.length,
                customDiagnostics: runs[0].diagnostics.custom.length,
              },
            ];
          }),
        ),
      ]),
    ),
  ]),
);

const report = {
  metadata: {
    date: new Date().toISOString(),
    mode,
    revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
    dirty:
      execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim()
        .length > 0,
    node: process.version,
    platform: `${platform()} ${arch()}`,
    cpu: cpus()[0]?.model,
    eslint: packageVersion("eslint-v9"),
    upstream: packageVersion("@graphql-eslint/eslint-plugin"),
    graphql: packageVersion("graphql"),
    analyzerPlugin: json(join(pluginDirectory, "package.json")).version,
    analyzerCore: json(join(root, "packages/core/package.json")).version,
    nativeBuild:
      process.env.BENCH_NATIVE_BUILD ?? "unverified (use the release build command in README)",
    nativeArtifacts,
    samples,
    repeats,
    warmups,
    scenarios,
    schemas: Object.fromEntries(corpora.map((name) => [name, manifest[name]])),
    operations: Object.fromEntries(
      corpora.map((name) => [name, json(join(projects, name, "corpus.json"))]),
    ),
  },
  coverage,
  ruleTests,
  mismatches,
  results,
};
mkdirSync(reportDirectory, { recursive: true });
const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
const output = join(reportDirectory, `${mode}-${timestamp}.json`);
writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
const lines = [
  `# ESLint ${mode} report`,
  "",
  `Date: ${report.metadata.date}`,
  `Revision: ${report.metadata.revision}${report.metadata.dirty ? " (dirty)" : ""}`,
  `Runner: ${report.metadata.cpu} (${report.metadata.platform}); Node ${report.metadata.node}; ESLint ${report.metadata.eslint}`,
  `Plugins: community ${report.metadata.upstream}; analyzer ${report.metadata.analyzerPlugin}; native build ${report.metadata.nativeBuild}`,
  `Samples: ${samples} fresh processes per lane; ${repeats} warm and changed-file repeats; ${warmups} warmups`,
  "",
  `Per-rule parity: ${ruleTests.status}. Corpus mismatches: ${mismatches.length}.`,
  "",
  `Active shared rules: ${coverage.activeShared.length}; validation stubs: ${coverage.validationStubs.length}; analyzer-only: ${coverage.analyzerOnly.length}; upstream-only: ${coverage.upstreamOnly.length}.`,
  "",
  "Validation stubs are configuration aliases; native validation diagnostics are outside rule-by-rule parity.",
  "",
  "Ratios divide the community median by the analyzer median. Values above 1 mean the analyzer was faster in this run; values below 1 mean it was slower.",
  "",
  "| Corpus | Scenario | Analyzer lane | Cold community / analyzer ms (ratio) | Warm community / analyzer ms (ratio) | Changed community / analyzer ms (ratio) |",
  "| --- | --- | --- | ---: | ---: | ---: |",
];
function comparison(base, candidate) {
  return `${base.median} / ${candidate.median} (${(base.median / candidate.median).toFixed(2)}×)`;
}
for (const name of corpora)
  for (const scenario of scenarios)
    for (const lane of scenarioLanes[scenario].filter(
      (candidate) => candidate !== "upstream" && candidate !== "upstream-custom",
    )) {
      const baseline =
        results[name][scenario][lane.endsWith("-custom") ? "upstream-custom" : "upstream"];
      const analyzer = results[name][scenario][lane];
      lines.push(
        `| ${name} | ${scenario} | ${lane} | ${comparison(baseline.coldProjectMs, analyzer.coldProjectMs)} | ${comparison(baseline.warmProjectMs, analyzer.warmProjectMs)} | ${comparison(baseline.changedLintMs, analyzer.changedLintMs)} |`,
      );
    }
lines.push(
  "",
  "Full measurements (median/p95):",
  "",
  "| Corpus | Scenario | Lane | Cold median/p95 ms | Warm median/p95 ms | Changed median/p95 ms | RSS median/p95 MiB |",
  "| --- | --- | --- | ---: | ---: | ---: | ---: |",
);
for (const name of corpora)
  for (const scenario of scenarios)
    for (const lane of scenarioLanes[scenario]) {
      const entry = results[name][scenario][lane];
      lines.push(
        `| ${name} | ${scenario} | ${lane} | ${entry.coldProjectMs.median}/${entry.coldProjectMs.p95} | ${entry.warmProjectMs.median}/${entry.warmProjectMs.p95} | ${entry.changedLintMs.median}/${entry.changedLintMs.p95} | ${entry.rssMiB.median}/${entry.rssMiB.p95} |`,
      );
    }
lines.push("", `Raw data: ${basename(output)} (included in the CI artifact)`, "");
const markdown = output.replace(/\.json$/, ".md");
writeFileSync(markdown, lines.join("\n"));
if (process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${lines.join("\n")}\n`);
}
console.log(`Reports: ${output} and ${markdown}`);
if (ruleTests.status === "failed" || mismatches.length || coverage.upstreamOnly.length) {
  console.error(
    `Parity failed: ${mismatches.length} corpus differences, ${coverage.upstreamOnly.length} missing rule names; see ${output}`,
  );
  process.exitCode = 1;
}
