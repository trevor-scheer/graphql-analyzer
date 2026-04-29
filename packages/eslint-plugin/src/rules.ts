import { createHash } from "crypto";
import type { Rule } from "eslint";
import * as binding from "./binding";

function toKebabCase(name: string): string {
  return name.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
}

// Per-file diagnostic cache keyed by filename + content hash + per-rule
// overrides hash. ESLint's rule machinery calls every rule's Program visitor
// for each file; without a cache the Rust `lint_file` would run once per
// (rule × file) instead of once per file. The content hash rules out
// stale-content collisions that a length-based key would miss (e.g.,
// successive `lintText` calls with same-length inputs).
//
// The overrides hash is part of the key so that different rules' option sets
// don't share a cache slot. Rules with no ESLint-config options (no second
// rule entry) all hit the same slot — common case stays one binding call per
// file. Each rule that DOES carry options triggers its own cache slot, so
// per-rule binding calls are O(rules-with-options) rather than O(rules).
const fileCache = new Map<string, binding.JsDiagnostic[]>();

function stableJson(value: unknown): string {
  // Stable serialization so option-equivalent objects with reordered keys
  // share a cache slot. Recurses through arrays and objects; primitives
  // round-trip via JSON.stringify.
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  const keys = Object.keys(value as Record<string, unknown>).sort();
  const entries = keys.map(
    (k) => `${JSON.stringify(k)}:${stableJson((value as Record<string, unknown>)[k])}`,
  );
  return `{${entries.join(",")}}`;
}

function cacheKey(filePath: string, source: string, overrides?: Record<string, unknown>): string {
  const overridesPart = overrides && Object.keys(overrides).length > 0 ? stableJson(overrides) : "";
  const digest = createHash("sha1").update(source).update("\0").update(overridesPart).digest("hex");
  return `${filePath}\0${digest}`;
}

function diagnosticsFor(
  filePath: string,
  source: string,
  overrides?: Record<string, unknown>,
): binding.JsDiagnostic[] {
  const key = cacheKey(filePath, source, overrides);
  const cached = fileCache.get(key);
  if (cached) return cached;
  const fresh = binding.lintFile(filePath, source, overrides);
  fileCache.set(key, fresh);
  return fresh;
}

// Per-rule override registry populated as ESLint instantiates each rule for a
// file (`create(context)`). ESLint calls every enabled rule's `create()`
// before any visitor method fires, so by the time the first `Program()`
// runs the registry already contains every rule's ESLint-config options
// for the current run. That lets the binding call use one merged overrides
// payload instead of one binding call per rule. Different ESLint runs may
// enable different rules, but options for a given rule are stable across
// files within a run, so a Map keyed by analyzer rule name converges to
// the right snapshot.
const overridesByRule = new Map<string, { severity: string; options?: unknown }>();

// Recursively convert any `RegExp` instances to their `.source` string. JS
// configs (e.g. `forbiddenPatterns: [/foo/i]`) carry RegExp instances; those
// get lost on `JSON.stringify` (RegExp serializes to `{}`), so we normalize
// them to the string form the Rust analyzer's `regex` crate accepts. The
// flag suffix is preserved when present (`(?i)foo` style) by prefixing
// `regex` syntax flags so the underlying regex still respects them.
function normalizeRegExps(value: unknown): unknown {
  if (value instanceof RegExp) {
    // The `regex` crate's syntax for inline flags is `(?<flags>:pattern)`.
    // JS flags map: `i` (case-insensitive), `m` (multi-line), `s` (dotall),
    // `u` and `y` are not relevant to pattern semantics here. Only inline
    // the flags we know map cleanly.
    const flags = value.flags
      .split("")
      .filter((f) => f === "i" || f === "m" || f === "s")
      .join("");
    return flags ? `(?${flags})${value.source}` : value.source;
  }
  if (Array.isArray(value)) return value.map(normalizeRegExps);
  if (value !== null && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(value as Record<string, unknown>)) {
      out[k] = normalizeRegExps((value as Record<string, unknown>)[k]);
    }
    return out;
  }
  return value;
}

function registerOverride(analyzerRuleName: string, options: unknown): void {
  // ESLint only invokes `create()` for rules enabled at warn or error
  // (level >= 1), so just registering forces the analyzer to enable the
  // rule. Severity here is the analyzer's "should I run this rule?" flag,
  // not the user-facing severity — ESLint stamps its own level on the
  // resulting messages. `"warn"` is the safe lower bound that always
  // enables the rule.
  const normalized = options !== undefined ? normalizeRegExps(options) : undefined;
  overridesByRule.set(analyzerRuleName, {
    severity: "warn",
    ...(normalized !== undefined ? { options: normalized } : {}),
  });
}

function buildOverridesPayload(): Record<string, unknown> | undefined {
  if (overridesByRule.size === 0) return undefined;
  const out: Record<string, unknown> = {};
  for (const [name, cfg] of overridesByRule) out[name] = cfg;
  return out;
}

// Rules where graphql-eslint reports a single-position `loc` (start only) so
// `endLine`/`endColumn` come back `undefined`. Our underlying diagnostic
// always carries a full range — useful for LSP/CLI consumers — but for these
// specific rules the eslint adapter strips the end so the message shape
// matches graphql-eslint exactly. Add a rule here only when graphql-eslint's
// own implementation is intentionally start-only (e.g. `no-hashtag-description`
// passes `loc: { line, column }` rather than `{ start, end }`).
//
// Exported so the parity test can derive upstream's actual behavior at
// runtime and assert this set still matches — drift on a graphql-eslint
// version bump becomes a CI failure rather than a silent regression.
export const START_ONLY_RULES = new Set([
  "noHashtagDescription",
  "requireSelections",
  "matchDocumentFilename",
  "selectionSetDepth",
]);

// Universally permissive options schema. graphql-eslint declares per-rule
// JSON Schemas (often `additionalProperties: false`); we don't need to
// duplicate those validators here because the Rust side already deserialises
// into typed structs and ignores unknown keys. Allowing any object lets users
// pass the same options graphql-eslint accepts (and a superset) without
// ESLint's flat-config validator rejecting calls to rules with options.
const OPTIONS_SCHEMA: Rule.RuleMetaData["schema"] = [
  { type: "object", additionalProperties: true },
];

// graphql-eslint emits ESLint-style autofixes (`fix`) for some rules and
// suggestion-only entries (`suggest`) for others; rules that wrap graphql-js
// validators (e.g. `no-unused-*`) carry neither. Listing the analyzer rule
// names that graphql-eslint surfaces a `fix` on lets us suppress our internal
// autofix payload for the rest, keeping ESLint output 1:1 with graphql-eslint.
// LSP/CLI consumers still see every fix via the diagnostic chain — only the
// ESLint `LintMessage.fix` view is filtered.
const ESLINT_FIXABLE_RULES = new Set(["alphabetize"]);

// graphql-eslint emits a stable `messageId` per diagnostic site so consumers
// can branch on `messageId` rather than parsing the human-readable message
// string. Our Rust rules carry the same id on `JsDiagnostic.messageId`; to
// surface it on ESLint's `LintMessage.messageId` we need ESLint to recognise
// the id, which means it must be a key in `meta.messages`. We don't know the
// id catalog until diagnostics are produced, so the strategy is:
//   1. Cache observed messageIds per analyzer rule across files.
//   2. Mutate `meta.messages` with a `{{ message }}` passthrough when a new id
//      shows up, then fall back to dynamic registration on the next visit.
// This is hacky but lets us preserve graphql-eslint's per-site ids without a
// separate catalog API at the napi boundary.
const seenMessageIds = new Map<string, Set<string>>();

function ensureMessageId(rule: Rule.RuleModule, analyzerRuleName: string, id: string): boolean {
  const messages = (rule.meta!.messages ??= {});
  if (id in messages) return true;
  let observed = seenMessageIds.get(analyzerRuleName);
  if (!observed) {
    observed = new Set();
    seenMessageIds.set(analyzerRuleName, observed);
  }
  observed.add(id);
  messages[id] = "{{ message }}";
  return true;
}

function makeRule(analyzerRuleName: string, description: string): Rule.RuleModule {
  const startOnly = START_ONLY_RULES.has(analyzerRuleName);
  const rule: Rule.RuleModule = {
    meta: {
      type: "problem",
      // ESLint refuses to apply a fix unless `meta.fixable` is set; declare
      // `"code"` for every rule so any underlying rule's autofix flows
      // through. Rules without fixes simply never produce a fix payload.
      fixable: "code",
      // ESLint refuses to surface `suggest` arrays unless `hasSuggestions`
      // is set. Declare `true` for every rule so the analyzer's suggestions
      // (when present on a diagnostic) flow through; rules whose Rust impl
      // doesn't emit suggestions just produce empty arrays.
      hasSuggestions: true,
      docs: { description },
      schema: OPTIONS_SCHEMA,
      messages: {},
    },
    create(context) {
      // Register this rule's ESLint-config options into the shared registry
      // so the eventual binding call carries every enabled rule's overrides
      // in one payload (one binding call per file, not per rule).
      registerOverride(analyzerRuleName, context.options[0]);
      return {
        Program() {
          const diagnostics = diagnosticsFor(
            context.filename,
            context.sourceCode.text,
            buildOverridesPayload(),
          );
          for (const d of diagnostics) {
            if (d.rule !== analyzerRuleName) continue;
            const loc = startOnly
              ? { line: d.line, column: d.column - 1 }
              : {
                  start: { line: d.line, column: d.column - 1 },
                  end: { line: d.endLine, column: d.endColumn - 1 },
                };
            // Materialize line-starts once per Program() visit so both fix
            // and suggestion fixers reuse the same offset table.
            const lineStarts = computeLineStarts(context.sourceCode.text);
            const buildFixer = (jsFix: binding.JsFix) => (fixer: Rule.RuleFixer) => {
              const edits = jsFix.edits.map((e) => ({
                range: [
                  lineStarts[e.rangeStartLine - 1] + (e.rangeStartColumn - 1),
                  lineStarts[e.rangeEndLine - 1] + (e.rangeEndColumn - 1),
                ] as [number, number],
                text: e.newText,
              }));
              if (edits.length === 1) {
                return fixer.replaceTextRange(edits[0].range, edits[0].text);
              }
              return edits.map((e) => fixer.replaceTextRange(e.range, e.text));
            };
            const fix =
              d.fix && ESLINT_FIXABLE_RULES.has(analyzerRuleName) ? buildFixer(d.fix) : undefined;
            // Suggestions are independent of the fix surface — every
            // analyzer-emitted suggestion routes through ESLint's `suggest`
            // array regardless of whether the rule also surfaces a `fix`.
            const suggest =
              d.suggestions && d.suggestions.length > 0
                ? d.suggestions.map((s) => ({
                    desc: s.desc,
                    fix: buildFixer(s.fix),
                  }))
                : undefined;
            const reportExtras = {
              ...(fix ? { fix } : {}),
              ...(suggest ? { suggest } : {}),
            };
            if (d.messageId) {
              ensureMessageId(rule, analyzerRuleName, d.messageId);
              context.report({
                messageId: d.messageId,
                data: { message: d.message },
                loc,
                ...reportExtras,
              });
            } else {
              context.report({ message: d.message, loc, ...reportExtras });
            }
          }
        },
      };
    },
  };
  return rule;
}

// Cache line-start byte offsets so fix edits can map (line, column) → byte
// range. Computed lazily per `Program()` visit since context.sourceCode.text
// is stable within a single lint pass.
function computeLineStarts(text: string): number[] {
  const starts = [0];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "\n") starts.push(i + 1);
  }
  return starts;
}

// Names of GraphQL spec validation rules upstream `@graphql-eslint` exposes
// as configurable lint rules. We run the same checks inside the analyzer's
// always-on validation pass, so configuring them is a no-op for us — but
// users migrating from upstream's preset configs (or custom configs that
// reference these names) shouldn't see "rule not found" errors. Each entry
// becomes a no-op rule module so configs load cleanly. The underlying
// validation diagnostics still fire as built-in errors regardless.
const VALIDATION_RULE_STUBS = [
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
];

function makeStubRule(ruleName: string): Rule.RuleModule {
  return {
    meta: {
      type: "problem",
      docs: {
        description:
          `GraphQL spec validation rule (\`${ruleName}\`). Always-on inside the ` +
          `analyzer's validation pass — this configurable shim is a no-op kept ` +
          `for drop-in compatibility with @graphql-eslint preset configs.`,
      },
      schema: OPTIONS_SCHEMA,
      messages: {},
    },
    create() {
      return {};
    },
  };
}

export function buildRules(): Record<string, Rule.RuleModule> {
  const rules: Record<string, Rule.RuleModule> = {};
  const meta = binding.getRules();

  for (const rule of meta) {
    const kebabName = toKebabCase(rule.name);
    rules[kebabName] = makeRule(rule.name, rule.description);
  }
  for (const stubName of VALIDATION_RULE_STUBS) {
    rules[stubName] = makeStubRule(stubName);
  }

  return rules;
}
