import assert from "node:assert/strict";
import { spawnSync, execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const script = fileURLToPath(import.meta.url);
const repository = path.resolve(path.dirname(script), "../../..");
const lanes = ["fast", "compatible", "compatible-custom", "upstream", "upstream-custom"];
const files = positiveInteger(process.env.BENCH_FILES, 50);
const samples = positiveInteger(process.env.BENCH_SAMPLES, 3);
const repeats = positiveInteger(process.env.BENCH_REPEATS, 3);
const analyzerEslint = process.env.BENCH_ANALYZER_ESLINT ?? "eslint-v9";
assert.ok(["eslint", "eslint-v9"].includes(analyzerEslint));

function positiveInteger(value, fallback) {
  const parsed = value === undefined ? fallback : Number(value);
  assert.ok(Number.isInteger(parsed) && parsed > 0, "benchmark sizes must be positive integers");
  return parsed;
}

function distribution(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
  return { median: round(median), min: round(sorted[0]), max: round(sorted.at(-1)) };
}

function round(value) {
  return Math.round(value * 100) / 100;
}

function signature(results, category = "native") {
  return results.flatMap((result) =>
    result.messages
      .filter((message) =>
        category === "native"
          ? message.ruleId?.startsWith("plugin/")
          : message.ruleId?.startsWith("custom/"),
      )
      .map(({ ruleId, messageId, message, line, column, endLine, endColumn, severity }) => ({
        ruleId,
        messageId,
        message,
        line,
        column,
        endLine,
        endColumn,
        severity,
      })),
  );
}

function checkMessages(results) {
  const errors = results.flatMap((result) =>
    result.messages.filter((message) => message.fatal || !message.ruleId),
  );
  assert.deepEqual(errors, [], "benchmark encountered a parser or configuration error");
}

async function child(lane, directory) {
  const start = performance.now();
  const upstream = lane.startsWith("upstream");
  const { ESLint } = require(upstream ? "eslint-v9" : analyzerEslint);
  const plugin = upstream
    ? require("@graphql-eslint/eslint-plugin")
    : require("../dist/index.js").default;
  const moduleLoadMs = performance.now() - start;
  const custom = lane.endsWith("-custom");
  const schemaRule = {
    meta: { schema: [], messages: { typed: "Field has type {{type}}" } },
    create(context) {
      return {
        Field(node) {
          const type = node.typeInfo().gqlType;
          assert.ok(type, "schema TypeInfo is required for every visited field");
          if (node.name.value === "f002")
            context.report({ node: node.name, messageId: "typed", data: { type: String(type) } });
        },
      };
    },
  };
  const linter = new ESLint({
    cwd: directory,
    overrideConfigFile: true,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: lane === "fast" ? plugin.fastParser : plugin.parser },
        plugins: {
          plugin,
          ...(custom ? { custom: { rules: { "schema-field": schemaRule } } } : {}),
        },
        rules: {
          "plugin/no-anonymous-operations": "error",
          "plugin/no-duplicate-fields": "error",
          ...(custom ? { "custom/schema-field": "warn" } : {}),
        },
      },
    ],
  });
  const filename = path.join(directory, "operations/operation000.graphql");
  const source = fs.readFileSync(filename, "utf8");
  async function timed(run) {
    const before = performance.now();
    const result = await run();
    const elapsed = performance.now() - before;
    checkMessages(result);
    return { result, elapsed };
  }
  const cold = await timed(() => linter.lintText(source, { filePath: filename }));
  const firstResultMs = performance.now() - start;
  const same = [];
  const changed = [];
  const changedSource = source.replace("f000 f000", "f000 f099");
  assert.equal(changedSource.length, source.length);
  let changedResult;
  for (let index = 0; index < repeats; index++) {
    const repeated = await timed(() => linter.lintText(source, { filePath: filename }));
    assert.deepEqual(signature(repeated.result), signature(cold.result));
    same.push(repeated.elapsed);
  }
  let previousSignature = signature(cold.result);
  for (let index = 0; index < repeats; index++) {
    const input = index % 2 === 0 ? changedSource : source;
    const updated = await timed(() => linter.lintText(input, { filePath: filename }));
    assert.notDeepEqual(
      signature(updated.result),
      previousSignature,
      "changed input must change native diagnostics",
    );
    previousSignature = signature(updated.result);
    changed.push(updated.elapsed);
    changedResult ??= updated.result;
  }
  const project = await timed(() => linter.lintFiles(["operations/*.graphql"]));
  assert.equal(project.result.length, files);
  const moduleNames = Object.keys(require.cache);
  const loadedCompatibility = moduleNames.some((filename) =>
    /node_modules\/(?:graphql\/|graphql-config\/|@graphql-tools\/graphql-tag-pluck\/)/.test(
      filename,
    ),
  );
  if (lane === "fast")
    assert.equal(loadedCompatibility, false, "fast mode loaded compatibility dependencies");
  return {
    lane,
    moduleLoadMs,
    firstResultMs,
    coldLintMs: cold.elapsed,
    sameLintMs: same,
    changedLintMs: changed,
    projectLintMs: project.elapsed,
    memoryMiB: {
      heap: process.memoryUsage().heapUsed / 2 ** 20,
      rss: process.memoryUsage().rss / 2 ** 20,
    },
    loadedCompatibility,
    signatures: {
      cold: signature(cold.result),
      changed: signature(changedResult),
      project: signature(project.result),
      customCold: signature(cold.result, "custom"),
      customProject: signature(project.result, "custom"),
    },
  };
}

function version(packageName) {
  const manifestName = packageName === "eslint-v9" ? "eslint" : packageName;
  let directory = path.dirname(require.resolve(packageName));
  while (directory !== path.dirname(directory)) {
    const manifest = path.join(directory, "package.json");
    if (fs.existsSync(manifest)) {
      const parsed = JSON.parse(fs.readFileSync(manifest, "utf8"));
      if (parsed.name === manifestName) return parsed.version;
    }
    directory = path.dirname(directory);
  }
  throw new Error(`Cannot find version of ${packageName}`);
}

async function main() {
  if (process.argv[2] === "--child") {
    console.log(JSON.stringify(await child(process.argv[3], process.argv[4])));
    return;
  }
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "graphql-eslint-benchmark-"));
  try {
    fs.mkdirSync(path.join(directory, "operations"));
    fs.writeFileSync(
      path.join(directory, "schema.graphql"),
      `type Query {\n${Array.from({ length: 100 }, (_, index) => `  f${String(index).padStart(3, "0")}: String`).join("\n")}\n}\n`,
    );
    fs.writeFileSync(
      path.join(directory, ".graphqlrc.json"),
      JSON.stringify({
        schema: "schema.graphql",
        documents: "operations/*.graphql",
        extensions: {
          "graphql-analyzer": {
            lint: { rules: { noAnonymousOperations: "error", noDuplicateFields: "error" } },
          },
        },
      }),
    );
    for (let index = 0; index < files; index++) {
      const id = String(index).padStart(3, "0");
      const fields = [
        "f000",
        "f000",
        ...Array.from({ length: 39 }, (_, field) => `f${String(field + 1).padStart(3, "0")}`),
      ];
      fs.writeFileSync(
        path.join(directory, `operations/operation${id}.graphql`),
        `query Operation${id} {\n  ${fields.join(" ")}\n}\n`,
      );
    }
    const observations = Object.fromEntries(lanes.map((lane) => [lane, []]));
    let expected;
    let expectedCustom;
    for (let sample = 0; sample < samples; sample++) {
      for (const lane of lanes) {
        const start = performance.now();
        const result = spawnSync(process.execPath, [script, "--child", lane, directory], {
          encoding: "utf8",
          env: process.env,
          maxBuffer: 16 * 1024 * 1024,
          timeout: 60_000,
        });
        assert.equal(result.status, 0, `${lane} failed: ${result.stderr}\n${result.stdout}`);
        const observation = JSON.parse(result.stdout);
        observation.processMs = performance.now() - start;
        const { customCold, customProject, ...nativeSignatures } = observation.signatures;
        expected ??= nativeSignatures;
        assert.deepEqual(nativeSignatures, expected, `${lane} diagnostic mismatch`);
        if (lane.endsWith("-custom")) {
          expectedCustom ??= { customCold, customProject };
          assert.deepEqual(
            { customCold, customProject },
            expectedCustom,
            `${lane} custom diagnostic mismatch`,
          );
        }
        observations[lane].push(observation);
      }
    }
    const results = Object.fromEntries(
      lanes.map((lane) => {
        const entries = observations[lane];
        return [
          lane,
          {
            moduleLoadMs: distribution(entries.map((entry) => entry.moduleLoadMs)),
            firstResultMs: distribution(entries.map((entry) => entry.firstResultMs)),
            coldLintMs: distribution(entries.map((entry) => entry.coldLintMs)),
            sameLintMs: distribution(entries.flatMap((entry) => entry.sameLintMs)),
            changedLintMs: distribution(entries.flatMap((entry) => entry.changedLintMs)),
            projectLintMs: distribution(entries.map((entry) => entry.projectLintMs)),
            processMs: distribution(entries.map((entry) => entry.processMs)),
            heapMiB: distribution(entries.map((entry) => entry.memoryMiB.heap)),
            rssMiB: distribution(entries.map((entry) => entry.memoryMiB.rss)),
            loadedCompatibility: entries[0].loadedCompatibility,
          },
        ];
      }),
    );
    console.log(
      JSON.stringify(
        {
          metadata: {
            date: new Date().toISOString(),
            node: process.version,
            eslint: version(analyzerEslint),
            upstreamEslint: version("eslint-v9"),
            upstream: version("@graphql-eslint/eslint-plugin"),
            graphql: version("graphql"),
            platform: `${os.platform()} ${os.arch()}`,
            cpu: os.cpus()[0]?.model,
            revision: execFileSync("git", ["rev-parse", "HEAD"], {
              cwd: repository,
              encoding: "utf8",
            }).trim(),
            workingTreeDirty:
              execFileSync("git", ["status", "--porcelain"], {
                cwd: repository,
                encoding: "utf8",
              }).trim().length > 0,
            files,
            fieldsPerOperation: 41,
            schemaFields: 100,
            samples,
            repeats,
            nativeBuild:
              process.env.BENCH_NATIVE_BUILD ??
              "unverified; build core in release mode before comparing timings",
            nativeArtifacts: fs
              .readdirSync(path.resolve(repository, "packages/core"))
              .filter((file) => file.endsWith(".node"))
              .map((file) => ({
                filename: file,
                sha256: createHash("sha256")
                  .update(fs.readFileSync(path.resolve(repository, "packages/core", file)))
                  .digest("hex"),
              })),
          },
          equivalence: {
            nativeDiagnosticsPerColdLint: expected.cold.length,
            nativeDiagnosticsChangedLint: expected.changed.length,
            nativeDiagnosticsProject: expected.project.length,
            customDiagnosticsPerColdLint: expectedCustom.customCold.length,
            customDiagnosticsProject: expectedCustom.customProject.length,
          },
          results,
        },
        null,
        2,
      ),
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

await main();
