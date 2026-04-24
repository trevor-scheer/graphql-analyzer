// Parity test vs @graphql-eslint/eslint-plugin.
//
// Enforces that:
//   1. Rules we already expose have the same name as graphql-eslint's
//      counterpart (drop-in migration is find-and-replace on the plugin name).
//   2. For rule names shared between the two plugins, both fire on the same
//      fixture files — the migration produces "equivalent" diagnostics, not
//      necessarily identical wording.
//
// Intentional gaps (see docs/src/content/docs/linting/eslint-plugin.mdx):
//   - Validation-category rules from graphql-js (`known-type-names`,
//     `fields-on-correct-type`, etc.) are not exposed as lint rules in
//     graphql-analyzer. They run as part of the analyzer's validation pass.
//     `KNOWN_MISSING` captures this so we fail CI when graphql-eslint adds a
//     non-validation rule we should have.
//   - A handful of linter-specific rules we have that graphql-eslint doesn't
//     (`operation-name-suffix`, `redundant-fields`, `require-id-field`, etc.).

import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import * as path from "node:path";
import { ESLint } from "eslint";

import ours from "../dist/index.js";
import theirsNs from "@graphql-eslint/eslint-plugin";

const theirs = theirsNs.default ?? theirsNs;
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixtureRoot = path.resolve(__dirname, "../../../test-workspace/eslint-migration");

// graphql-eslint rules we intentionally don't ship. Keep this list
// alphabetized and note the reason.
const KNOWN_MISSING = new Set([
  // GraphQL-spec validation rules (from graphql-js `specifiedRules`). They
  // run inside the analyzer's validation pass rather than as configurable
  // lint rules.
  "executable-definitions",
  "fields-on-correct-type",
  "fragments-on-composite-type",
  "known-argument-names",
  "known-directives",
  "known-fragment-names",
  "known-type-names",
  "lone-anonymous-operation",
  "lone-schema-definition",
  "no-fragment-cycles",
  "no-undefined-variables",
  "one-field-subscriptions",
  "overlapping-fields-can-be-merged",
  "possible-fragment-spread",
  "possible-type-extension",
  "provided-required-arguments",
  "scalar-leafs",
  "unique-argument-names",
  "unique-directive-names",
  "unique-directive-names-per-location",
  "unique-field-definition-names",
  "unique-fragment-name",
  "unique-input-field-names",
  "unique-operation-name",
  "unique-operation-types",
  "unique-type-names",
  "unique-variable-names",
  "value-literals-of-correct-type",
  "variables-are-input-types",
  "variables-in-allowed-position",
  // Naming mismatch tracked separately (see KNOWN_NAMING_MISMATCH below) —
  // treat these as "not present under graphql-eslint's name" so the strict
  // parity check still has a signal.
  "no-unused-fields",
  "no-unused-fragments",
  "no-unused-variables",
]);

// Our rule name -> graphql-eslint rule name. Pre-publish follow-up: rename
// these on the Rust side so migration truly is a find-and-replace.
const KNOWN_NAMING_MISMATCH = new Map([
  ["unused-fields", "no-unused-fields"],
  ["unused-fragments", "no-unused-fragments"],
  ["unused-variables", "no-unused-variables"],
]);

// Rules we ship that graphql-eslint doesn't. OK to extend the surface, but
// surprising additions should be deliberate — the allowlist catches
// accidental ones.
const KNOWN_EXTRA = new Set([
  "operation-name-suffix",
  "redundant-fields",
  "require-id-field",
  "unique-names",
  ...KNOWN_NAMING_MISMATCH.keys(),
]);

function theirRules() {
  return new Set(Object.keys(theirs.rules ?? {}));
}

function ourRules() {
  return new Set(Object.keys(ours.rules ?? {}));
}

test("no unexpected missing rules vs graphql-eslint", () => {
  const missing = [...theirRules()]
    .filter((r) => !ourRules().has(r) && !KNOWN_MISSING.has(r))
    .sort();
  assert.deepEqual(
    missing,
    [],
    `graphql-eslint has these rules we don't — add them or add to KNOWN_MISSING with a reason:\n  ${missing.join("\n  ")}`,
  );
});

test("no unexpected extra rules vs graphql-eslint", () => {
  const extra = [...ourRules()].filter((r) => !theirRules().has(r) && !KNOWN_EXTRA.has(r)).sort();
  assert.deepEqual(
    extra,
    [],
    `our plugin has rules graphql-eslint doesn't — add to KNOWN_EXTRA if intentional:\n  ${extra.join("\n  ")}`,
  );
});

test("rules shared with graphql-eslint fire on the same fixture files", async () => {
  // Build both plugins against the same fixture config so differences are
  // attributable to analyzer behavior, not config drift.
  const sharedRules = [...ourRules()].filter((r) => theirRules().has(r));
  assert.ok(sharedRules.length > 0, "expected at least one shared rule");

  // Only assert on rules that the fixture project actually exercises, so we
  // don't rely on rules firing in empty files.
  const exercised = {
    "no-anonymous-operations": { file: "src/operations.graphql", severity: 2 },
    "no-duplicate-fields": { file: "src/operations.graphql", severity: 2 },
    "no-hashtag-description": { file: "schema.graphql", severity: 1 },
  };

  for (const [rule, { file, severity }] of Object.entries(exercised)) {
    const ourDiag = await lintOne("ours", rule, severity, file);
    const theirDiag = await lintOne("theirs", rule, severity, file);

    assert.ok(ourDiag.length > 0, `our plugin didn't fire ${rule} on ${file}`);
    assert.ok(
      theirDiag.length > 0,
      `graphql-eslint didn't fire ${rule} on ${file} — fixture may need updating`,
    );
  }
});

async function lintOne(which, rule, severity, file) {
  const plugin = which === "ours" ? ours : theirs;
  const scope = which === "ours" ? "@graphql-analyzer" : "@graphql-eslint";
  const eslint = new ESLint({
    overrideConfigFile: true,
    cwd: fixtureRoot,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { [scope]: plugin },
        rules: { [`${scope}/${rule}`]: severity },
      },
    ],
  });
  const [result] = await eslint.lintFiles([file]);
  return result.messages.filter((m) => m.ruleId === `${scope}/${rule}`);
}
