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
//   - GraphQL spec validation rules (`known-type-names`, `fields-on-correct-type`,
//     etc.) always run inside the analyzer's validation pass. We expose them
//     as no-op stub rules (`STUB_RULES`) so users migrating upstream's preset
//     configs don't see "rule not found" errors — but the configurable shim
//     never fires, since the underlying check is always-on.
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

// Rules we ship that graphql-eslint doesn't.
const KNOWN_EXTRA = new Set([
  "operation-name-suffix",
  "redundant-fields",
  "require-id-field",
  "unique-names",
]);

// Rules we expose as no-op stubs purely for drop-in config compatibility
// with `@graphql-eslint`'s preset configs that reference GraphQL spec
// validation rules by name. The underlying check still runs as built-in
// validation; the stub just stops ESLint from erroring with "rule not
// found" when a user pastes upstream's `flat/schema-recommended` rules
// list. Excluded from the shared-rule parity assertions for the same
// reason — there's nothing to compare diagnostics-wise (they're no-ops).
const STUB_RULES = new Set([
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

function theirRules() {
  return new Set(Object.keys(theirs.rules ?? {}));
}

function ourRules() {
  return new Set(Object.keys(ours.rules ?? {}));
}

test("no unexpected missing rules vs graphql-eslint", () => {
  // Every upstream rule should be present in our plugin — either as an
  // implemented rule or as a no-op stub (`STUB_RULES`) for spec validation
  // rules that always run as built-in validation. New upstream additions
  // need a deliberate decision: implement, or stub.
  const missing = [...theirRules()].filter((r) => !ourRules().has(r)).sort();
  assert.deepEqual(
    missing,
    [],
    `graphql-eslint has these rules we don't — implement them or add to STUB_RULES (and the plugin's stub list) with a reason:\n  ${missing.join("\n  ")}`,
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
    // Suggestion: `Rename to \`hello\`` inserts the operation name after
    // the `query` keyword (or `query <name> ` for shorthand `{`).
    compareSuggest: true,
  },

  "no-duplicate-fields": {
    files: {
      "schema.graphql": "type Query { user: User } type User { id: ID! email: String }\n",
      "src/op.graphql": "query Q { user { id email id } }\n",
    },
    target: "src/op.graphql",
    severity: 2,
    span: "line",
    // Suggestion: `Remove \`id\` field` deletes the duplicate selection.
    compareSuggest: true,
  },

  "no-hashtag-description": {
    // Multiple `#` lines on adjacent rows attached to the same definition
    // count as ONE comment block (graphql-eslint groups; we group). A
    // separate `#` block at file scope (gap row before the next definition)
    // is its own diagnostic. Single line covered indirectly by the second
    // attached comment.
    files: {
      "schema.graphql":
        "# A note about the schema.\n" +
        "\n" +
        "# Represents a user\n" +
        "# with a name\n" +
        "type User { name: String }\n" +
        "\n" +
        "# A query type\n" +
        "type Query { user: User }\n",
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
    // Suggestion: `Change to block style description` rewraps `"A type"`
    // → `"""A type"""` (or vice versa), matching upstream byte-for-byte.
    compareSuggest: true,
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
    // Suggestion: `Make String nullable` strips the `!` from the gqlType,
    // mirroring upstream's `text.replace("!", "")` on the full type ref.
    compareSuggest: true,
  },

  // ----- newly verified rules (this PR) -----

  alphabetize: {
    // Exercises the schema-side modes: `definitions` (top-level type
    // ordering), `fields` per-kind narrowing (object type field order),
    // and `values` (enum value order). Messages, positions, AND fix
    // payloads match upstream byte-for-byte (the schema-side swap fix
    // mirrors the operation-side `swap_fix` shape).
    options: {
      definitions: true,
      fields: ["ObjectTypeDefinition", "InterfaceTypeDefinition", "InputObjectTypeDefinition"],
      values: true,
    },
    files: {
      "schema.graphql":
        "type Query { hello: String }\n" +
        "type Zebra { name: String age: Int }\n" +
        "type Apple { id: ID! }\n" +
        "enum Role { SUPER_ADMIN ADMIN USER GOD }\n",
    },
    target: "schema.graphql",
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
    // Suggestion: `Rename to \`input\`` replaces the argument's Name token.
    compareSuggest: true,
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
    // Exercises:
    // - per-kind object form (`OperationDefinition: { style, forbiddenPrefixes }`)
    // - the ESLint selector form `FieldDefinition[parent.name.value=Query]`
    //   that upstream's `flat/schema-recommended` uses
    // - the comma-list selector form `EnumTypeDefinition,EnumTypeExtension`
    // The schema-only fixture isolates the schema-side enforcement so the
    // diagnostic ordering is stable. Variable-name casing and the
    // `forbiddenPatterns` shape mismatch are covered by Rust unit tests.
    options: {
      "FieldDefinition[parent.name.value=Query]": {
        forbiddenPrefixes: ["query", "get"],
      },
      "EnumTypeDefinition,EnumTypeExtension": {
        forbiddenPrefixes: ["Enum"],
      },
    },
    files: {
      "schema.graphql":
        "type Query { getUser: User queryAll: [User] hello: String } " +
        "type User { id: ID! getName: String } " +
        "enum EnumRole { ADMIN }\n",
    },
    target: "schema.graphql",
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
    // Suggestion: `Remove Field` deletes the entire deprecated Field
    // selection (matches upstream's `fixer.remove(node)`).
    compareSuggest: true,
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
    // Suggestion: `Remove \`Mutation\` type` deletes the full type def.
    compareSuggest: true,
  },

  "no-scalar-result-type-on-mutation": {
    files: {
      "schema.graphql":
        "type Query { ok: Boolean }\n" + "type Mutation {\n  deleteUser: Boolean!\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
    // Suggestion: `Remove \`Boolean\`` deletes the named-type token (matches
    // upstream's `fixer.remove(node)` on the NamedType node).
    compareSuggest: true,
  },

  "no-typename-prefix": {
    files: {
      "schema.graphql":
        "type Query { user: User }\ntype User {\n  userId: ID!\n  name: String\n}\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
    // Suggestion: `Remove \`User\` prefix` rewrites `userId` → `Id`,
    // matching upstream's case-insensitive prefix strip.
    compareSuggest: true,
  },

  "no-unreachable-types": {
    files: {
      "schema.graphql":
        "type Query { me: User }\n" + "type User { id: ID! }\n" + "type Orphan { name: String }\n",
    },
    target: "schema.graphql",
    severity: 1,
    span: "full",
    // Suggestion: `Remove \`Orphan\`` deletes the full type def.
    compareSuggest: true,
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
    // Exercises both `maxDepth` and `ignore`: a query that *would* exceed
    // depth 1 but for the field `b` being ignored. Both plugins should
    // produce zero diagnostics — `b` is treated as a leaf and its subtree
    // isn't counted.
    options: { maxDepth: 1, ignore: ["b"] },
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
    // Suggestion: `Remove \`VALUE\` enum value` (and similarly for the
    // third duplicate) deletes the value's full def range.
    compareSuggest: true,
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
    // Suggestion: `Remove \`unusedField\` field` deletes the field's
    // entire definition range, matching upstream byte-for-byte.
    compareSuggest: true,
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
  // decision on first sight. Stubs are excluded — they're no-op shims for
  // drop-in compatibility, not active rules with diagnostics to compare.
  const shared = [...ourRules()].filter((r) => theirRules().has(r) && !STUB_RULES.has(r));
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
      const drift = parityDiff(ourDiag, theirDiag, cfg.span, cfg.skipFix, cfg.compareSuggest);
      if (drift) errors.push(`${rule}: ${drift}`);
    });
  }
  assert.deepEqual(errors, [], `parity drift:\n  ${errors.join("\n  ")}`);
});

// Verifies rule options forwarded *only* through ESLint's `rules:` config
// reach the analyzer (i.e. ESLint config alone is enough; .graphqlrc.yaml
// doesn't need a duplicate `lint.rules` entry). The default `withProject`
// helper writes options to *both* channels, so this test isolates the
// ESLint channel by writing a `.graphqlrc.yaml` with no `lint.rules` block.
//
// Picks rules whose EXERCISED fixture sets `options` — without options we
// have no way to distinguish "options forwarded" from "rule defaulted". The
// rule must (a) fire at all (proving our analyzer enables it from the
// ESLint config) and (b) fire identically to upstream (proving the options
// payload arrived intact).
test("ESLint-config rule options reach the analyzer (no .graphqlrc.yaml lint block)", async () => {
  const errors = [];
  for (const [rule, cfg] of Object.entries(EXERCISED)) {
    if (cfg.options === undefined) continue;
    await withProjectNoLintBlock(rule, cfg, async (root) => {
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
      if (ourDiag.length === 0 && theirDiag.length > 0) {
        errors.push(
          `${rule}: theirs fired (${theirDiag.length}) but ours didn't — ` +
            `options not reaching analyzer through ESLint config alone`,
        );
        return;
      }
      const drift = parityDiff(ourDiag, theirDiag, cfg.span, cfg.skipFix, cfg.compareSuggest);
      if (drift) errors.push(`${rule}: ${drift}`);
    });
  }
  assert.deepEqual(errors, [], `ESLint-options-only parity drift:\n  ${errors.join("\n  ")}`);
});

// Sibling of `withProject` that writes a `.graphqlrc.yaml` carrying *only*
// the schema/documents wiring, with no `lint.rules` block. The rule must
// reach the analyzer through the ESLint config payload alone.
async function withProjectNoLintBlock(rule, cfg, fn) {
  const root = mkdtempSync(path.join(tmpdir(), `parity-eslint-${rule}-`));
  try {
    for (const [relpath, content] of Object.entries(cfg.files)) {
      const abs = path.join(root, relpath);
      mkdirSync(path.dirname(abs), { recursive: true });
      writeFileSync(abs, content);
    }
    const lines = [];
    if (cfg.files["schema.graphql"]) lines.push(`schema: "schema.graphql"`);
    const docs = Object.keys(cfg.files).filter((p) => p.startsWith("src/"));
    if (docs.length > 0) lines.push(`documents: "src/**/*"`);
    writeFileSync(path.join(root, ".graphqlrc.yaml"), lines.join("\n") + "\n");
    return await fn(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

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
  const observed = new Set(); // rules where upstream produced ≥1 diagnostic
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
      observed.add(camelOf(rule));
      // Upstream is start-only for this rule iff *all* its diagnostics omit
      // endLine. (A rule that mixes shapes would itself be a parity concern.)
      const allStartOnly = theirDiag.every((d) => d.endLine === undefined || d.endLine === null);
      if (allStartOnly) observedStartOnly.add(camelOf(rule));
    });
  }
  // Compare only on the observable subset — rules whose fixtures intentionally
  // produce zero diagnostics (e.g. `selection-set-depth` exercising `ignore`)
  // can't have their loc shape verified, so we can't enforce membership for
  // them either way. The full set still has to round-trip when at least one
  // diagnostic surfaces.
  const expectedObservable = [...START_ONLY_RULES].filter((r) => observed.has(r)).sort();
  const observedSorted = [...observedStartOnly].sort();
  assert.deepEqual(
    observedSorted,
    expectedObservable,
    `START_ONLY_RULES drift vs upstream's actual loc shape:\n` +
      `  ours (observable subset): ${JSON.stringify(expectedObservable)}\n` +
      `  upstream:                 ${JSON.stringify(observedSorted)}\n` +
      `Update src/rules.ts:START_ONLY_RULES to match.`,
  );
});

function parityDiff(ours, theirs, span, skipFix = false, compareSuggest = false) {
  if (ours.length !== theirs.length) {
    return `count drift: ours=${ours.length} theirs=${theirs.length}`;
  }
  const oursCanon = canonical(ours, span, skipFix, compareSuggest);
  const theirsCanon = canonical(theirs, span, skipFix, compareSuggest);
  if (JSON.stringify(oursCanon) !== JSON.stringify(theirsCanon)) {
    const flags = [`span=${span}`, skipFix && "skipFix", compareSuggest && "compareSuggest"]
      .filter(Boolean)
      .join(", ");
    return (
      `diff (${flags})\n` +
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
// `compareSuggest` is opt-in per-fixture: only fixtures that explicitly
// expect suggestions enable it (the default `false` keeps the 33+ existing
// no-suggestion fixtures from regressing as suggestion implementations
// land per-rule).
function canonical(diagnostics, span, skipFix = false, compareSuggest = false) {
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
        ...(skipFix ? {} : { fix: d.fix ? { range: d.fix.range, text: d.fix.text } : null }),
        ...(compareSuggest
          ? {
              suggestions: (d.suggestions ?? []).map((s) => ({
                desc: s.desc,
                ...(s.fix ? { fix: { range: s.fix.range, text: s.fix.text } } : {}),
              })),
            }
          : {}),
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
