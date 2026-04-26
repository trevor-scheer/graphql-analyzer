// Parity test vs @graphql-eslint/eslint-plugin.
//
// Enforces that:
//   1. Rules we already expose have the same name as graphql-eslint's
//      counterpart (drop-in migration is find-and-replace on the plugin name).
//   2. Every shared rule fires identically on a per-rule isolated fixture —
//      same diagnostic count, same messages, same source positions.
//
// Each rule in EXERCISED carries its own fixture (inline strings); the
// runner builds a throwaway project per probe, writes the fixture, runs both
// plugins with the named rule enabled, and compares. This keeps fixtures from
// cross-firing (rule X's fixture won't accidentally trip rule Y's parity
// test) and means adding a new rule's coverage doesn't perturb existing
// fixtures.
//
// The "every shared rule is parity-verified" test below fails if a rule is
// added to either plugin without landing in EXERCISED. So new shared rules
// force a parity decision on first sight.
//
// Intentional gaps (see docs/src/content/docs/linting/eslint-plugin.mdx):
//   - Validation-category rules from graphql-js (`known-type-names`,
//     `fields-on-correct-type`, etc.) run inside the analyzer's validation
//     pass, not as configurable lint rules. `KNOWN_MISSING` captures these
//     so we fail CI when graphql-eslint adds a non-validation rule we should
//     have.
//   - A handful of linter-specific rules we have that graphql-eslint doesn't
//     (`operation-name-suffix`, `redundant-fields`, `require-id-field`, etc.).

import test from "node:test";
import assert from "node:assert/strict";
import { ESLint } from "eslint";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import * as path from "node:path";

import ours from "../dist/index.js";
import { START_ONLY_RULES } from "../dist/rules.js";
import theirsNs from "@graphql-eslint/eslint-plugin";

const theirs = theirsNs.default ?? theirsNs;
const __dirname = path.dirname(fileURLToPath(import.meta.url));
// graphql-eslint caches its parsed `graphQLConfig` at module scope. Running
// each `theirs` probe in a fresh child process bypasses the cache and
// prevents rule-N's project state from leaking into rule-(N+1)'s diagnostics.
const THEIRS_RUNNER = path.join(__dirname, "_lint-theirs.mjs");

// graphql-eslint rules we intentionally don't ship.
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
]);

// Rules we ship that graphql-eslint doesn't.
const KNOWN_EXTRA = new Set([
  "operation-name-suffix",
  "redundant-fields",
  "require-id-field",
  "unique-names",
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

// Convert kebab-case (rule names) to camelCase (analyzer config keys).
const camelCase = (s) => s.replace(/-([a-z])/g, (_, c) => c.toUpperCase());

// Each entry describes a per-rule isolated fixture and the parity assertion
// applied to the resulting diagnostics.
//
//   files: { "<relpath>": "<contents>", ... }
//     The set of files written into the throwaway project. `schema.graphql` is
//     loaded as the project schema; everything under `src/` is treated as a
//     document. At least one file must be the lint target (`target`).
//
//   target: "<relpath>"
//     The single file we run ESLint against — diagnostics are filtered by
//     `ruleId === <scope>/<rule>` so this rule's parity is measured in
//     isolation even when a fixture also trips other rules.
//
//   options: <any> (optional)
//     Passed as the rule's option payload to BOTH plugins. Use this when
//     graphql-eslint's rule schema requires options or when the only sensible
//     parity is at non-default options. Ours must accept the same shape.
//
//   severity: 1 | 2
//
//   span: "line" | "full"
//     "full" compares { line, column, endLine, endColumn }; "line" only
//     compares the firing line. Use "full" wherever both plugins produce
//     well-defined ranges.
const EXERCISED = {
  // ----- already-verified rules from prior PRs (#1008, #1011, #1012, #1013, #1014) -----

  "no-anonymous-operations": {
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/op.graphql": "query { hello }\nquery { hello }\n",
    },
    target: "src/op.graphql",
    severity: 2,
    span: "line",
  },

  "no-duplicate-fields": {
    files: {
      "schema.graphql": "type Query { user: User } type User { id: ID! email: String }\n",
      "src/op.graphql": "query Q { user { id email id } }\n",
    },
    target: "src/op.graphql",
    severity: 2,
    span: "line",
  },

  "no-hashtag-description": {
    files: {
      "schema.graphql": "# Don't use this as a description\ntype Query { hello: String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "description-style": {
    files: {
      "schema.graphql": '"A type"\ntype Query { id: ID! }\n',
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-field-of-type-query-in-mutation-result": {
    files: {
      "schema.graphql":
        "type Query { hello: String }\n" +
        "type Mutation { createUser: CreateUserResult }\n" +
        "type CreateUserResult { user: User }\n" +
        "type User { id: ID! }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-nullable-result-in-root": {
    files: {
      "schema.graphql": "type Query { version: String! }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  // ----- newly verified rules (this PR) -----

  alphabetize: {
    // graphql-eslint requires explicit options (`minProperties: 1`).
    options: { selections: ["OperationDefinition"] },
    files: {
      "schema.graphql": "type Query { user: User } type User { id: ID! name: String! age: Int }\n",
      "src/op.graphql": "query Q { user { name age id } }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "input-name": {
    files: {
      "schema.graphql":
        "type Query { _: Boolean }\n" + "type Mutation { setMessage(message: String): String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "lone-executable-definition": {
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/op.graphql": "query A { hello }\nquery B { hello }\nfragment F on Query { hello }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "match-document-filename": {
    options: { query: "PascalCase" },
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/operation.graphql": "query SomethingElse { hello }\n",
    },
    target: "src/operation.graphql",
    severity: 1,
    span: "line",
  },

  "naming-convention": {
    // Both plugins no-op without explicit kind config; this fixture just
    // exercises that the no-config behavior matches.
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/op.graphql": "query lowercaseOp { hello }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "no-deprecated": {
    files: {
      "schema.graphql":
        "type Query { user: User }\n" +
        "type User {\n" +
        "  id: ID!\n" +
        '  oldField: String @deprecated(reason: "use newField")\n' +
        "  newField: String\n" +
        "}\n",
      "src/op.graphql": "query GetUser { user { id oldField } }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "no-one-place-fragments": {
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/op.graphql": "fragment F on Query { hello }\nquery Q { ...F }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "no-root-type": {
    options: { disallow: ["mutation", "subscription"] },
    files: {
      "schema.graphql":
        "type Query { hello: String }\n" + "type Mutation { setHello(s: String): String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "no-scalar-result-type-on-mutation": {
    files: {
      "schema.graphql":
        "type Query { ok: Boolean }\n" + "type Mutation {\n  deleteUser: Boolean!\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "no-typename-prefix": {
    files: {
      "schema.graphql":
        "type Query { user: User }\ntype User {\n  userId: ID!\n  name: String\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "no-unreachable-types": {
    files: {
      "schema.graphql":
        "type Query { me: User }\n" + "type User { id: ID! }\n" + "type Orphan { name: String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "relay-arguments": {
    files: {
      "schema.graphql":
        "type Query {\n  posts: PostConnection\n}\n\n" +
        "type PostConnection { edges: [PostEdge] pageInfo: PageInfo! }\n" +
        "type PostEdge { node: Post cursor: String! }\n" +
        "type Post { id: ID! }\n" +
        "type PageInfo { hasPreviousPage: Boolean! hasNextPage: Boolean! startCursor: String endCursor: String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "relay-connection-types": {
    files: {
      "schema.graphql":
        "type Query { users: UserConnection }\n\n" +
        "type UserConnection {\n  edges: UserEdge\n  pageInfo: PageInfo\n}\n\n" +
        "type UserEdge { node: User cursor: String! }\n" +
        "type User { id: ID! }\n" +
        "type PageInfo { hasPreviousPage: Boolean! hasNextPage: Boolean! startCursor: String endCursor: String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "relay-edge-types": {
    files: {
      "schema.graphql":
        "type Query { users: UserConnection }\n\n" +
        "type UserConnection { edges: [UserItem] pageInfo: PageInfo! }\n\n" +
        "type UserItem {\n  cursor: String!\n}\n\n" +
        "type User { id: ID! }\n" +
        "type PageInfo { hasPreviousPage: Boolean! hasNextPage: Boolean! startCursor: String endCursor: String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "relay-page-info": {
    files: {
      "schema.graphql":
        "type Query { hello: String }\n\n" +
        "type PageInfo {\n  hasPreviousPage: Boolean\n  hasNextPage: String\n  startCursor: String\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-deprecation-date": {
    files: {
      "schema.graphql":
        "type Query { _: Boolean }\n" +
        "type User {\n" +
        "  id: ID!\n" +
        '  oldField: String @deprecated(reason: "use newField")\n' +
        "}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-deprecation-reason": {
    files: {
      "schema.graphql":
        "type Query { _: Boolean }\n" +
        "type User {\n  id: ID!\n  oldField: String @deprecated\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-description": {
    options: { types: true },
    files: {
      "schema.graphql": "type Query { _: Boolean }\n\ntype Undocumented {\n  id: ID!\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-import-fragment": {
    files: {
      "schema.graphql": "type Query { user: User } type User { id: ID! name: String }\n",
      "src/op.graphql": "query GetUser { user { ...UserFields } }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "require-nullable-fields-with-oneof": {
    files: {
      "schema.graphql":
        "directive @oneOf on INPUT_OBJECT | OBJECT\n" +
        "type Query { _: Boolean }\n" +
        "input Foo @oneOf {\n  a: String!\n  b: Int\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "require-selections": {
    files: {
      "schema.graphql": "type Query { user: User }\n" + "type User { id: ID! name: String! }\n",
      "src/op.graphql": "query Q { user { name } }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "require-type-pattern-with-oneof": {
    files: {
      "schema.graphql":
        "directive @oneOf on INPUT_OBJECT | OBJECT\n" +
        "type Query { _: Boolean }\n" +
        "type Result @oneOf {\n  success: String\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "selection-set-depth": {
    options: { maxDepth: 2 },
    files: {
      "schema.graphql":
        "type Query { a: A } type A { b: B } type B { c: C } type C { d: String }\n",
      "src/op.graphql": "query Q {\n  a { b { c { d } } }\n}\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "line",
  },

  "strict-id-in-types": {
    files: {
      "schema.graphql": "type Query { user: User }\ntype User {\n  name: String!\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "unique-enum-value-names": {
    files: {
      "schema.graphql":
        "type Query { e: MyEnum }\n" + "enum MyEnum {\n  Value\n  VALUE\n  ValuE\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "no-unused-fields": {
    files: {
      "schema.graphql":
        "type Query { user: User }\n" +
        "type User {\n  id: ID!\n  name: String\n  unusedField: String\n}\n",
      "src/op.graphql": "query Q { user { id name } }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
  },

  "no-unused-fragments": {
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/op.graphql": "fragment Unused on Query { hello }\nquery Q { hello }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },

  "no-unused-variables": {
    files: {
      "schema.graphql": "type Query { hello: String }\n",
      "src/op.graphql": "query Q($name: String) { hello }\n",
    },
    target: "src/op.graphql",
    severity: 1,
    span: "full",
  },
};

test("every shared rule is parity-verified", () => {
  // Inverts the default: any shared rule must appear in EXERCISED with a
  // verified parity assertion. New rules added to either plugin force a
  // decision on first sight.
  const shared = [...ourRules()].filter((r) => theirRules().has(r));
  const exercised = new Set(Object.keys(EXERCISED));

  const unaccounted = shared.filter((r) => !exercised.has(r)).sort();
  assert.deepEqual(
    unaccounted,
    [],
    "Shared rules without parity coverage. Add to EXERCISED with a fixture:\n  " +
      unaccounted.join("\n  "),
  );

  const stale = [...exercised].filter((r) => !shared.includes(r)).sort();
  assert.deepEqual(
    stale,
    [],
    `EXERCISED entries that aren't shared rules — remove them:\n  ${stale.join("\n  ")}`,
  );
});

test("messages, counts, and source positions match graphql-eslint exactly", async () => {
  // Hard parity: same diagnostic count, same messages, same positions
  // (granularity per cfg.span). All rules are checked; failures are
  // collected so a single failing rule doesn't mask drifts in others.
  //
  // Both plugins lint the SAME tmp project per rule, so messages that
  // happen to embed the document path (e.g. `no-one-place-fragments`'s
  // "Inline him in '<path>'.") match between the two runs.
  const errors = [];
  for (const [rule, cfg] of Object.entries(EXERCISED)) {
    await withProject(rule, cfg, async (root) => {
      let ourDiag, theirDiag;
      try {
        ourDiag = await lintInProject(root, ours, "@graphql-analyzer", rule, cfg);
      } catch (err) {
        errors.push(`${rule}: ours threw: ${err.message.split("\n")[0]}`);
        return;
      }
      try {
        theirDiag = lintTheirsInChild(root, rule, cfg);
      } catch (err) {
        errors.push(`${rule}: theirs threw: ${err.message.split("\n")[0]}`);
        return;
      }
      if (ourDiag.length === 0 && theirDiag.length === 0) {
        // Both fire zero — trivially at parity. (Some rules no-op at default
        // options on purpose, e.g. `naming-convention` requires explicit
        // kind config in both plugins.)
        return;
      }
      if (ourDiag.length > 0 && theirDiag.length === 0) {
        errors.push(`${rule}: ours fired (${ourDiag.length}) but theirs didn't`);
        return;
      }
      if (theirDiag.length > 0 && ourDiag.length === 0) {
        errors.push(`${rule}: theirs fired (${theirDiag.length}) but ours didn't`);
        return;
      }
      const drift = parityDiff(ourDiag, theirDiag, cfg.span);
      if (drift) errors.push(`${rule}: ${drift}`);
    });
  }
  assert.deepEqual(errors, [], `parity drift:\n  ${errors.join("\n  ")}`);
});

// Drift guard: graphql-eslint reports start-only `loc` (no endLine/endColumn)
// for a few rules, and our shim manually strips end positions for the same
// list (`START_ONLY_RULES` in `src/rules.ts`). When upstream changes a rule's
// loc shape, that hand-curated list goes stale silently — until enough time
// passes that someone notices the parity test using `span: "full"` is now
// catching unrelated diffs.
//
// This test runs upstream against every EXERCISED fixture, observes which
// diagnostics arrive with `endLine === undefined`, and asserts the inferred
// set of "upstream is start-only" rules matches our hardcoded
// `START_ONLY_RULES`. If upstream tightens or loosens a rule's loc shape on
// a version bump, this test fails first with the rule named.
test("START_ONLY_RULES tracks upstream's actual start-only rule set", async () => {
  const camelOf = (kebab) => kebab.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const observedStartOnly = new Set();
  for (const [rule, cfg] of Object.entries(EXERCISED)) {
    await withProject(rule, cfg, async (root) => {
      let theirDiag;
      try {
        theirDiag = lintTheirsInChild(root, rule, cfg);
      } catch {
        return;
      }
      if (theirDiag.length === 0) return;
      // Upstream is start-only for this rule iff *all* its diagnostics omit
      // endLine. (A rule that mixes shapes would itself be a parity concern.)
      const allStartOnly = theirDiag.every(
        (d) => d.endLine === undefined || d.endLine === null,
      );
      if (allStartOnly) observedStartOnly.add(camelOf(rule));
    });
  }
  const expected = [...START_ONLY_RULES].sort();
  const observed = [...observedStartOnly].sort();
  assert.deepEqual(
    observed,
    expected,
    `START_ONLY_RULES drift vs upstream's actual loc shape:\n` +
      `  ours:      ${JSON.stringify(expected)}\n` +
      `  upstream:  ${JSON.stringify(observed)}\n` +
      `Update src/rules.ts:START_ONLY_RULES to match.`,
  );
});

function parityDiff(ours, theirs, span) {
  if (ours.length !== theirs.length) {
    return `count drift: ours=${ours.length} theirs=${theirs.length}`;
  }
  const oursCanon = canonical(ours, span);
  const theirsCanon = canonical(theirs, span);
  if (JSON.stringify(oursCanon) !== JSON.stringify(theirsCanon)) {
    return (
      `diff (span=${span})\n` +
      `    ours:   ${JSON.stringify(oursCanon)}\n` +
      `    theirs: ${JSON.stringify(theirsCanon)}`
    );
  }
  return null;
}

// Build a canonical, sorted representation of the diagnostics so that two
// diagnostics are paired by position (not independently sorted by message).
// Compares position, message, messageId, and fix together — drift in any one
// surfaces as a difference. Source positions narrow to `line` only when
// `span === "line"` (graphql-eslint reports start-only loc for some rules).
function canonical(diagnostics, span) {
  return diagnostics
    .map((d) => {
      const pos =
        span === "full"
          ? { line: d.line, column: d.column, endLine: d.endLine, endColumn: d.endColumn }
          : { line: d.line };
      return {
        ...pos,
        message: d.message,
        messageId: d.messageId ?? null,
        fix: d.fix ? { range: d.fix.range, text: d.fix.text } : null,
      };
    })
    .sort((a, b) => {
      if (a.line !== b.line) return a.line - b.line;
      const ac = a.column ?? 0;
      const bc = b.column ?? 0;
      if (ac !== bc) return ac - bc;
      return a.message.localeCompare(b.message);
    });
}

async function withProject(rule, cfg, fn) {
  const root = mkdtempSync(path.join(tmpdir(), `parity-${rule}-`));
  try {
    // Write fixture files.
    for (const [relpath, content] of Object.entries(cfg.files)) {
      const abs = path.join(root, relpath);
      mkdirSync(path.dirname(abs), { recursive: true });
      writeFileSync(abs, content);
    }

    // Write `.graphqlrc.yaml` so the analyzer binding fires the rule and
    // graphql-config resolves the project (graphql-eslint relies on this
    // to know which file is schema vs document).
    const lines = [];
    if (cfg.files["schema.graphql"]) lines.push(`schema: "schema.graphql"`);
    const docs = Object.keys(cfg.files).filter((p) => p.startsWith("src/"));
    if (docs.length > 0) lines.push(`documents: "src/**/*"`);
    lines.push(`extensions:`);
    lines.push(`  graphql-analyzer:`);
    lines.push(`    lint:`);
    lines.push(`      rules:`);
    if (cfg.options !== undefined) {
      lines.push(`        ${camelCase(rule)}:`);
      lines.push(`          - warn`);
      lines.push(`          - ${JSON.stringify(cfg.options)}`);
    } else {
      lines.push(`        ${camelCase(rule)}: warn`);
    }
    writeFileSync(path.join(root, ".graphqlrc.yaml"), lines.join("\n") + "\n");

    return await fn(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

async function lintInProject(root, plugin, scope, rule, cfg) {
  const ruleEntry = cfg.options !== undefined ? [cfg.severity, cfg.options] : cfg.severity;
  const eslint = new ESLint({
    overrideConfigFile: true,
    cwd: root,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { [scope]: plugin },
        rules: { [`${scope}/${rule}`]: ruleEntry },
      },
    ],
  });
  const [r] = await eslint.lintFiles([cfg.target]);
  return r.messages.filter((m) => m.ruleId === `${scope}/${rule}`);
}

function lintTheirsInChild(root, rule, cfg) {
  const optionsRaw = cfg.options !== undefined ? JSON.stringify(cfg.options) : "undefined";
  const out = execFileSync(
    process.execPath,
    [THEIRS_RUNNER, root, rule, cfg.target, String(cfg.severity), optionsRaw],
    { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
  );
  return JSON.parse(out);
}
