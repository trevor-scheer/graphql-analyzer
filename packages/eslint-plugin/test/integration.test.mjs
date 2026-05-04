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
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { ESLint } from "eslint";

import plugin from "../dist/index.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixtureRoot = path.resolve(__dirname, "../../../test-workspace/eslint-migration");

// Fail fast with an actionable error when build artifacts are missing, rather
// than letting the addon-load error bubble up as a cryptic MODULE_NOT_FOUND.
test("build artifacts exist", () => {
  assert.ok(
    fs.existsSync(path.resolve(__dirname, "../dist/index.js")),
    "run `pnpm --filter @graphql-analyzer/eslint-plugin run build` first",
  );
  const coreDir = path.resolve(__dirname, "../../core");
  const nodeBinaryExists = fs.readdirSync(coreDir).some((f) => f.endsWith(".node"));
  assert.ok(nodeBinaryExists, "run `pnpm --filter @graphql-analyzer/core run build:debug` first");
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

test("every preset loads without ESLint validation errors", async () => {
  // Catches typos in rule names, options the rule schema rejects, or
  // missing stubs. Each preset gets attached to a no-op fixture and
  // ESLint's flat-config validator runs over the full rule set.
  const root = mkdtempSync(path.join(tmpdir(), "preset-load-"));
  try {
    writeFileSync(path.join(root, ".graphqlrc.yaml"), 'schema: "schema.graphql"\n');
    writeFileSync(path.join(root, "schema.graphql"), "type Query { hello: String }\n");
    for (const presetName of Object.keys(plugin.configs)) {
      const eslint = new ESLint({
        overrideConfigFile: true,
        cwd: root,
        overrideConfig: [
          {
            files: ["**/*.graphql"],
            languageOptions: { parser: plugin.parser },
            plugins: { "@graphql-analyzer": plugin },
            rules: plugin.configs[presetName].rules,
          },
        ],
      });
      // Just calling lintFiles is enough — flat config validates rule
      // metadata and option schemas during the first pass.
      await eslint.lintFiles(["schema.graphql"]);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("plugin exposes expected shape", () => {
  assert.equal(typeof plugin.parser.parseForESLint, "function");
  assert.equal(typeof plugin.processor.preprocess, "function");
  assert.equal(typeof plugin.processor.postprocess, "function");
  assert.ok(Object.keys(plugin.rules).length > 0, "plugin must expose rules");
  // Mirror upstream's full preset set so existing imports keep working.
  for (const name of [
    "flat/schema-recommended",
    "flat/schema-all",
    "flat/schema-relay",
    "flat/operations-recommended",
    "flat/operations-all",
  ]) {
    assert.ok(plugin.configs[name], `${name} preset must exist`);
  }
});

test("processor extracts embedded GraphQL from JS/TS-family files", () => {
  const tsx = `import { gql } from "@apollo/client";\nconst Q = gql\`query { __typename }\`;\n`;
  const preprocessed = plugin.processor.preprocess(tsx, "component.tsx");
  // Expect [extractedBlock, originalSource]. ESLint matches the block's
  // `.graphql` filename against the user's `**/*.graphql` config block to
  // dispatch our parser/rules; the original source goes to whatever parser
  // the user has wired for `.tsx`.
  assert.equal(preprocessed.length, 2, "should return one block + the original source");
  assert.equal(typeof preprocessed[0], "object");
  assert.match(preprocessed[0].filename, /\.graphql$/);
  assert.equal(preprocessed[0].text, "query { __typename }");
  assert.equal(preprocessed[1], tsx);
});

test("processor postprocess remaps line offsets back to host coords", () => {
  const tsx =
    `import { gql } from "@apollo/client";\n` +
    `\n` +
    `const Q = gql\`\n` +
    `  query { __typename }\n` +
    `\`;\n`;
  plugin.processor.preprocess(tsx, "remap.tsx");
  const merged = plugin.processor.postprocess(
    [[{ ruleId: "@graphql-analyzer/no-anonymous-operations", line: 1, column: 3 }], []],
    "remap.tsx",
  );
  assert.equal(merged.length, 1);
  assert.ok(
    merged[0].line >= 3,
    `expected remap to host line ≥3 (block sits inside the gql template that opens on line 3); got ${merged[0].line}`,
  );
});

// Verifies the doc claim ("detects embedded GraphQL in TypeScript, JavaScript,
// Vue, Svelte, and Astro files") end-to-end through ESLint. Two-block config
// is required and matches the documented usage: the `.graphql` block applies
// our parser/rules to virtual blocks the processor emits; the `.tsx` block
// wires the processor on the host file. ESLint joins the host filename and
// the virtual block name with `/` (e.g. `component.tsx/0_document.graphql`),
// which matches the `**/*.graphql` pattern.
test("fires no-anonymous-operations on embedded GraphQL in .js", async () => {
  // .js so espree parses the host without error and isolates the
  // embedded-extraction concern from any JSX/TS parser concerns.
  const eslint = new ESLint({
    overrideConfigFile: true,
    cwd: fixtureRoot,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { "@graphql-analyzer": plugin },
        rules: {
          "@graphql-analyzer/no-anonymous-operations": "error",
        },
      },
      {
        files: ["**/*.js"],
        plugins: { "@graphql-analyzer": plugin },
        processor: "@graphql-analyzer/graphql",
      },
    ],
  });
  const results = await eslint.lintFiles(["src/embedded.js"]);
  const diags = results[0].messages.filter(
    (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
  );
  assert.ok(
    diags.length >= 1,
    `expected ≥1 no-anonymous-operations diagnostic in embedded.js; got ${diags.length}\n` +
      `messages: ${JSON.stringify(results[0].messages, null, 2)}`,
  );
  // The anonymous `query {` token sits on line 4 of `src/embedded.js`.
  assert.ok(diags[0].line >= 3, `expected embedded position remap; got line ${diags[0].line}`);
});

// SFC formats (`.vue`, `.svelte`, `.astro`) flow through the same processor
// path. The host parse via espree will fail (espree can't parse SFC syntax)
// and surface a fatal "Parsing error" diagnostic, but that's separate from
// the GraphQL extraction we care about here — filtering to
// `@graphql-analyzer/*` rule ids isolates the embedded-GraphQL contract.
// Users in real projects pair these blocks with the matching SFC parser
// (`vue-eslint-parser`, `svelte-eslint-parser`, `astro-eslint-parser`); we
// don't take a devDep on those here just to assert the extraction works.
//
// These tests sit BEFORE the multi-project test below because the addon's
// `init()` is global and the multi-project test points it at a tmpdir
// that's torn down after the test — leaving the in-memory analyzer state
// pointing at a path that no longer exists. Running our SFC checks first
// keeps us on the eslint-migration `.graphqlrc.yaml` (which has the rules
// we depend on enabled) for the duration of this assertion.
function sfcLinter() {
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
        },
      },
      {
        files: ["**/*.{vue,svelte,astro}"],
        plugins: { "@graphql-analyzer": plugin },
        processor: "@graphql-analyzer/graphql",
      },
    ],
  });
}

test("fires no-anonymous-operations on embedded GraphQL in .vue", async () => {
  const results = await sfcLinter().lintFiles(["src/component.vue"]);
  const diags = results[0].messages.filter(
    (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
  );
  assert.ok(
    diags.length >= 1,
    `expected ≥1 no-anonymous-operations diagnostic in component.vue; got ${diags.length}\n` +
      `messages: ${JSON.stringify(results[0].messages, null, 2)}`,
  );
  // The anonymous `query` token sits on line 5 of the host; remap should
  // place the diagnostic at or after that line.
  assert.ok(diags[0].line >= 4, `expected line remap into <script>; got line ${diags[0].line}`);
});

test("fires no-anonymous-operations on embedded GraphQL in .svelte", async () => {
  const results = await sfcLinter().lintFiles(["src/component.svelte"]);
  const diags = results[0].messages.filter(
    (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
  );
  assert.ok(
    diags.length >= 1,
    `expected ≥1 no-anonymous-operations diagnostic in component.svelte; got ${diags.length}\n` +
      `messages: ${JSON.stringify(results[0].messages, null, 2)}`,
  );
  assert.ok(diags[0].line >= 4, `expected line remap into <script>; got line ${diags[0].line}`);
});

test("fires no-anonymous-operations on embedded GraphQL in .astro", async () => {
  const results = await sfcLinter().lintFiles(["src/page.astro"]);
  const diags = results[0].messages.filter(
    (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
  );
  assert.ok(
    diags.length >= 1,
    `expected ≥1 no-anonymous-operations diagnostic in page.astro; got ${diags.length}\n` +
      `messages: ${JSON.stringify(results[0].messages, null, 2)}`,
  );
  // Anonymous `query` in the frontmatter sits on line 5 of the host.
  assert.ok(diags[0].line >= 4, `expected line remap into frontmatter; got line ${diags[0].line}`);
});

// SFC autofix: prove fix-range remapping works through the SFC byte offset.
// Uses `alphabetize` (one of the rules with full autofix support) inside a
// Svelte `<script>` block — applying `--fix` should reorder the fields
// inside the gql template without disturbing the surrounding markup.
test("autofix remaps fix range correctly in .svelte host", async () => {
  const root = mkdtempSync(path.join(tmpdir(), "sfc-fix-"));
  try {
    writeFileSync(path.join(root, ".graphqlrc.yaml"), 'schema: "schema.graphql"\n');
    writeFileSync(
      path.join(root, "schema.graphql"),
      "type Query { user(id: ID!): User }\ntype User { id: ID! name: String email: String }\n",
    );
    const before =
      `<script lang="ts">\n` +
      `  import { gql } from "graphql-tag";\n` +
      `  const Q = gql\`query GetUser { user(id: "1") { name id } }\`;\n` +
      `</script>\n` +
      `<p>hi</p>\n`;
    writeFileSync(path.join(root, "Test.svelte"), before);

    const eslint = new ESLint({
      overrideConfigFile: true,
      cwd: root,
      fix: true,
      overrideConfig: [
        {
          files: ["**/*.graphql"],
          languageOptions: { parser: plugin.parser },
          plugins: { "@graphql-analyzer": plugin },
          rules: {
            "@graphql-analyzer/alphabetize": ["error", { selections: ["OperationDefinition"] }],
          },
        },
        {
          files: ["**/*.svelte"],
          plugins: { "@graphql-analyzer": plugin },
          processor: "@graphql-analyzer/graphql",
        },
      ],
    });
    const [result] = await eslint.lintFiles(["Test.svelte"]);
    // ESLint only sets `output` when at least one fix was applied.
    assert.ok(result.output, "expected SFC autofix to produce output");
    assert.match(
      result.output,
      /id name/,
      `alphabetize should reorder the gql body fields in-place; got:\n${result.output}`,
    );
    // Markup outside the <script> block must be left untouched.
    assert.match(result.output, /<p>hi<\/p>/);
    assert.match(result.output, /<\/script>/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

// Multi-project `.graphqlrc.yaml`: two projects in the same workspace,
// each with its own schema and `lint.rules` block. The plugin must route
// each file to the matching project so the per-project lint config takes
// effect. Mirrors graphql-config + graphql-eslint behavior.
test("multi-project .graphqlrc routes files to matching project", async () => {
  const root = mkdtempSync(path.join(tmpdir(), "multi-proj-"));
  try {
    mkdirSync(path.join(root, "projA"), { recursive: true });
    mkdirSync(path.join(root, "projB"), { recursive: true });
    writeFileSync(path.join(root, "projA", "schema.graphql"), "type Query { hello: String }\n");
    writeFileSync(path.join(root, "projB", "schema.graphql"), "type Query { world: String }\n");
    writeFileSync(path.join(root, "projA", "op.graphql"), "query { hello }\nquery { hello }\n");
    writeFileSync(path.join(root, "projB", "op.graphql"), "query Named { world }\n");

    // ProjA enables no-anonymous-operations as error.
    // ProjB doesn't enable it — its file has a named query anyway, so even
    // without the rule we expect no diagnostics from it. The contrast is
    // what proves per-project routing: projA's file fires, projB's doesn't.
    writeFileSync(
      path.join(root, ".graphqlrc.yaml"),
      [
        "projects:",
        "  projA:",
        '    schema: "projA/schema.graphql"',
        '    documents: "projA/**/*.graphql"',
        "    extensions:",
        "      graphql-analyzer:",
        "        lint:",
        "          rules:",
        "            noAnonymousOperations: error",
        "  projB:",
        '    schema: "projB/schema.graphql"',
        '    documents: "projB/**/*.graphql"',
        "    extensions:",
        "      graphql-analyzer:",
        "        lint:",
        "          rules:",
        "            noAnonymousOperations: error",
        "",
      ].join("\n"),
    );

    const eslint = new ESLint({
      overrideConfigFile: true,
      cwd: root,
      overrideConfig: [
        {
          files: ["**/*.graphql"],
          languageOptions: { parser: plugin.parser },
          plugins: { "@graphql-analyzer": plugin },
          rules: {
            "@graphql-analyzer/no-anonymous-operations": "error",
          },
        },
      ],
    });

    const [resA] = await eslint.lintFiles(["projA/op.graphql"]);
    const anonA = resA.messages.filter(
      (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
    );
    assert.equal(
      anonA.length,
      2,
      `projA's two anonymous queries should fire; got ${anonA.length}: ${JSON.stringify(resA.messages)}`,
    );

    const [resB] = await eslint.lintFiles(["projB/op.graphql"]);
    const anonB = resB.messages.filter(
      (m) => m.ruleId === "@graphql-analyzer/no-anonymous-operations",
    );
    assert.equal(
      anonB.length,
      0,
      `projB's named query should produce no anonymous-op diagnostics; got ${JSON.stringify(resB.messages)}`,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

// `.tsx`/`.ts` extraction goes through the same processor path verified by
// the `.js` test above, but ESLint can't lint the host source without a
// parser that understands the host's syntax (espree can't parse JSX/TS).
// Users must wire e.g. `@typescript-eslint/parser` in a matching config
// block; that's a host-side concern documented in
// `docs/.../eslint-plugin.mdx`. We don't add a devDep on
// `@typescript-eslint/parser` here just to assert that.
