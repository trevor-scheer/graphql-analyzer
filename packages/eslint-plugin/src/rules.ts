import type { Rule, SourceCode } from "eslint";
import * as binding from "./binding";
import { embeddedDiagnostics } from "./embedded";
import { lineStarts as computeLineStarts } from "./source-map";
import { isCompatibilityProgram } from "./parser";

function toKebabCase(name: string): string {
  return name.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
}

interface FileState {
  overrides: Record<string, { severity: string; options?: unknown }>;
  diagnostics?: binding.JsDiagnostic[];
}

// A SourceCode belongs to one lint pass, including its complete enabled rule set.
const fileStates = new WeakMap<SourceCode, FileState>();

function stateFor(source: SourceCode): FileState {
  let state = fileStates.get(source);
  if (!state) {
    state = { overrides: {} };
    fileStates.set(source, state);
  }
  return state;
}

function diagnosticsFor(filePath: string, source: SourceCode): binding.JsDiagnostic[] {
  const state = stateFor(source);
  if (state.diagnostics) return state.diagnostics;
  // ESLint must see native reports to track which GraphQL directives are used.
  const skipEslintSuppressions =
    isCompatibilityProgram(source.ast) &&
    source.getAllComments().some((comment) =>
      /^\s*eslint-(?:disable(?:-next-line|-line)?|enable)(?:\s|$)/u.test(comment.value),
    );
  state.diagnostics ??=
    embeddedDiagnostics(filePath, binding.lintFile, state.overrides, skipEslintSuppressions) ??
    binding.lintFile(filePath, source.text, state.overrides, skipEslintSuppressions);
  return state.diagnostics;
}

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

function registerOverride(source: SourceCode, analyzerRuleName: string, options: unknown): void {
  const normalized = options !== undefined ? normalizeRegExps(options) : undefined;
  // ESLint owns report severity; warn enables the native rule for this pass.
  stateFor(source).overrides[analyzerRuleName] = {
    severity: "warn",
    ...(normalized !== undefined ? { options: normalized } : {}),
  };
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
      registerOverride(context.sourceCode, analyzerRuleName, context.options[0]);
      return {
        Program() {
          const diagnostics = diagnosticsFor(context.filename, context.sourceCode);
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

// These names let GraphQL-ESLint presets load without enabling validation checks.
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
          `Accepts the GraphQL-ESLint rule name \`${ruleName}\` in configuration. ` +
          `This rule does not report validation diagnostics.`,
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
