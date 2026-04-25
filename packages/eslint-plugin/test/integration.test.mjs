// Integration test for @graphql-analyzer/eslint-plugin.
//
// Runs ESLint programmatically against the eslint-migration fixture project
// and asserts the produced diagnostic set. Catches regressions in:
//   - rule shim wiring (rules dispatched by name)
//   - config auto-discovery from .graphqlrc.yaml
//   - napi addon loading and diagnostic round-trip
//
// Prereqs (verified at startup below): `@graphql-analyzer/core` has been
// built (`build:debug`) and the plugin has been compiled (`dist/` exists).

import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import * as fs from "node:fs";
import * as path from "node:path";
import { ESLint } from "eslint";

import plugin from "../dist/index.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixtureRoot = path.resolve(__dirname, "../../../test-workspace/eslint-migration");

// Fail fast with an actionable error when build artifacts are missing, rather
// than letting the addon-load error bubble up as a cryptic MODULE_NOT_FOUND.
test("build artifacts exist", () => {
  assert.ok(
    fs.existsSync(path.resolve(__dirname, "../dist/index.js")),
    "run `npm run build --workspace=@graphql-analyzer/eslint-plugin` first",
  );
  const coreDir = path.resolve(__dirname, "../../core");
  const nodeBinaryExists = fs.readdirSync(coreDir).some((f) => f.endsWith(".node"));
  assert.ok(nodeBinaryExists, "run `npm run build:debug --workspace=@graphql-analyzer/core` first");
});

function eslint() {
  return new ESLint({
    overrideConfigFile: true,
    cwd: fixtureRoot,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { "@graphql-analyzer": plugin },
        rules: {
          "@graphql-analyzer/no-anonymous-operations": "error",
          "@graphql-analyzer/no-duplicate-fields": "error",
          "@graphql-analyzer/no-hashtag-description": "warn",
        },
      },
    ],
  });
}

test("fires no-hashtag-description on schema comment", async () => {
  const results = await eslint().lintFiles(["schema.graphql"]);
  const diags = results[0].messages.filter(
    (m) => m.ruleId === "@graphql-analyzer/no-hashtag-description",
  );
  assert.ok(diags.length >= 1, `expected at least one no-hashtag-description, got ${diags.length}`);
  assert.equal(diags[0].severity, 1, "should report as warning");
});

test("fires no-anonymous-operations + no-duplicate-fields on ops file", async () => {
  const results = await eslint().lintFiles(["src/operations.graphql"]);
  const ruleIds = new Set(results[0].messages.map((m) => m.ruleId));
  assert.ok(
    ruleIds.has("@graphql-analyzer/no-anonymous-operations"),
    "expected no-anonymous-operations",
  );
  assert.ok(ruleIds.has("@graphql-analyzer/no-duplicate-fields"), "expected no-duplicate-fields");

  const anon = results[0].messages.find(
    (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
  );
  assert.equal(anon.severity, 2, "anonymous op should be error");
  assert.equal(anon.line, 1, "anonymous op should report at line 1");
});

test("produces no diagnostics on valid GraphQL", async () => {
  const valid = `query GetUser { user(id: "1") { id name } }\n`;
  const results = await eslint().lintText(valid, {
    filePath: path.join(fixtureRoot, "src/valid.graphql"),
  });
  const diags = results[0].messages.filter((m) => m.ruleId?.startsWith("@graphql-analyzer/"));
  assert.equal(diags.length, 0, `expected 0 plugin diagnostics, got ${diags.length}`);
});

test("plugin exposes expected shape", () => {
  assert.equal(typeof plugin.parser.parseForESLint, "function");
  assert.equal(typeof plugin.processor.preprocess, "function");
  assert.equal(typeof plugin.processor.postprocess, "function");
  assert.ok(Object.keys(plugin.rules).length > 0, "plugin must expose rules");
  assert.ok(plugin.configs["flat/schema-recommended"], "flat/schema-recommended preset must exist");
  assert.ok(
    plugin.configs["flat/operations-recommended"],
    "flat/operations-recommended preset must exist",
  );
});

test("processor is an identity passthrough for JS/TS-family files", () => {
  const tsx = `import { gql } from "@apollo/client";\nconst Q = gql\`query { __typename }\`;\n`;
  const preprocessed = plugin.processor.preprocess(tsx, "component.tsx");
  assert.deepEqual(
    preprocessed,
    [tsx],
    "preprocess should return original source unchanged until embedded-position remap is wired",
  );

  const merged = plugin.processor.postprocess([[{ ruleId: "x", line: 1 }]], "component.tsx");
  assert.equal(merged.length, 1);
});
