import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const [lane, project, repeatsRaw, warmupsRaw, scenario = "operations"] = process.argv.slice(2);
const repeats = Number(repeatsRaw);
const warmups = Number(warmupsRaw);
const start = performance.now();
const requirePlugin = createRequire(
  resolve(dirname(fileURLToPath(import.meta.url)), "../../packages/eslint-plugin/package.json"),
);
const { ESLint } = requirePlugin("eslint-v9");
const upstream = lane.startsWith("upstream");
const custom = lane.endsWith("-custom");
const pluginModule = upstream
  ? requirePlugin("@graphql-eslint/eslint-plugin")
  : requirePlugin("./dist/index.js");
const plugin = pluginModule.default ?? pluginModule;
const moduleLoadMs = performance.now() - start;
const workdir =
  scenario === "schema"
    ? join(project, "schema-workload")
    : scenario === "embedded"
      ? join(project, "embedded-workload")
      : project;
const pattern =
  scenario === "schema"
    ? "schema.graphql"
    : scenario === "embedded"
      ? "embedded/workload.js"
      : "operations/*.graphql";
const target =
  scenario === "schema"
    ? join(workdir, "schema.graphql")
    : scenario === "embedded"
      ? join(workdir, "embedded/workload.js")
      : join(workdir, "operations/operation-000.graphql");
const validPath =
  scenario === "schema"
    ? join(workdir, "valid.graphql")
    : scenario === "embedded"
      ? join(workdir, "embedded/valid.js")
      : join(workdir, "valid/operation-000.graphql");

const customRule = {
  meta: { schema: [], messages: { typed: "Field has type {{type}}" } },
  create(context) {
    return {
      Field(node) {
        if (node.name.value !== "__typename") return;
        const type = node.typeInfo().gqlType;
        assert.ok(type, "custom rule requires schema TypeInfo");
        context.report({ node: node.name, messageId: "typed", data: { type: String(type) } });
      },
    };
  },
};

const graphqlConfig = {
  files: ["**/*.graphql"],
  languageOptions: { parser: lane === "fast" ? plugin.fastParser : plugin.parser },
  plugins: {
    bench: plugin,
    ...(custom ? { custom: { rules: { typed: customRule } } } : {}),
  },
  rules: {
    ...(scenario === "schema"
      ? { "bench/no-hashtag-description": "error" }
      : { "bench/no-anonymous-operations": "error", "bench/no-duplicate-fields": "error" }),
    ...(custom ? { "custom/typed": "warn" } : {}),
  },
};
const linter = new ESLint({
  cwd: workdir,
  overrideConfigFile: true,
  overrideConfig:
    scenario === "embedded"
      ? [{ files: ["**/*.js"], processor: plugin.processor }, graphqlConfig]
      : [graphqlConfig],
});

function diagnostics(results, category = "bench/") {
  return results
    .flatMap((result) =>
      result.messages
        .filter((message) => message.ruleId?.startsWith(category))
        .map((message) => ({
          file: basename(result.filePath),
          rule: message.ruleId.slice(category.length),
          severity: message.severity,
          message: message.message,
          line: message.line,
          column: message.column,
          endLine: message.endLine ?? null,
          endColumn: message.endColumn ?? null,
          fix: message.fix ? { range: message.fix.range, text: message.fix.text } : null,
          suggestions: (message.suggestions ?? []).map((suggestion) => ({
            desc: suggestion.desc,
            fix: suggestion.fix ? { range: suggestion.fix.range, text: suggestion.fix.text } : null,
          })),
        })),
    )
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}

function checked(results) {
  const errors = results.flatMap((result) =>
    result.messages.filter((message) => message.fatal || !message.ruleId),
  );
  assert.deepEqual(errors, [], "ESLint parser/configuration error");
  return results;
}

async function measure(run) {
  const before = performance.now();
  const result = checked(await run());
  return { result, ms: performance.now() - before };
}

const first = await measure(() => linter.lintFiles([pattern]));
const firstResultMs = performance.now() - start;
const fileCount =
  scenario === "operations"
    ? Number(JSON.parse(readFileSync(join(project, "corpus.json"), "utf8")).operations)
    : 1;
assert.equal(first.result.length, fileCount, "project file count drift");
const warmProjectMs = [];
let peakRss = process.memoryUsage().rss;
for (let index = 0; index < warmups + repeats; index++) {
  const observation = await measure(() => linter.lintFiles([pattern]));
  if (index >= warmups) warmProjectMs.push(observation.ms);
  peakRss = Math.max(peakRss, process.memoryUsage().rss);
  assert.deepEqual(
    diagnostics(observation.result),
    diagnostics(first.result),
    "warm diagnostics changed",
  );
}

const filename = target;
const duplicate = readFileSync(target, "utf8");
const valid = readFileSync(validPath, "utf8");
const duplicateDiagnostics = diagnostics(first.result).filter(
  (message) => message.file === basename(filename),
);
const changedLintMs = [];
let validDiagnostics;
for (let index = 0; index < repeats * 2; index++) {
  const source = index % 2 ? duplicate : valid;
  const observation = await measure(() => linter.lintText(source, { filePath: filename }));
  const current = diagnostics(observation.result);
  if (index === 0) validDiagnostics = current;
  assert.deepEqual(
    current,
    index % 2 ? duplicateDiagnostics : validDiagnostics,
    "changed-source diagnostics became stale",
  );
  changedLintMs.push(observation.ms);
  peakRss = Math.max(peakRss, process.memoryUsage().rss);
}
assert.notDeepEqual(
  validDiagnostics,
  duplicateDiagnostics,
  "edited source must change diagnostics",
);
assert.ok(duplicateDiagnostics.length > 0, "benchmark fixture must produce native diagnostics");

const modules = Object.keys(requirePlugin.cache);
const loadedCompatibility = modules.some((filename) =>
  /node_modules\/(?:graphql\/|graphql-config\/|@graphql-tools\/graphql-tag-pluck\/)/.test(filename),
);
if (lane === "fast") {
  assert.equal(loadedCompatibility, false, "fast mode loaded compatibility dependencies");
}

console.log(
  JSON.stringify({
    lane,
    scenario,
    timings: { moduleLoadMs, firstResultMs, coldProjectMs: first.ms, warmProjectMs, changedLintMs },
    rssMiB: peakRss / 2 ** 20,
    loadedCompatibility,
    diagnostics: {
      project: diagnostics(first.result),
      custom: diagnostics(first.result, "custom/"),
      valid: validDiagnostics,
    },
  }),
);
