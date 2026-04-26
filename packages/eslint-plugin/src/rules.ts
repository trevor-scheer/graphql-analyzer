import { createHash } from "crypto";
import type { Rule } from "eslint";
import * as binding from "./binding";

function toKebabCase(name: string): string {
  return name.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
}

// Per-file diagnostic cache keyed by filename + content hash. ESLint's rule
// machinery calls every rule's Program visitor for each file, so without this
// the Rust `lint_file` runs once per (rule × file) instead of once per file.
// The hash rules out stale-content collisions that a length-based key would
// miss (e.g., successive `lintText` calls with same-length inputs).
const fileCache = new Map<string, binding.JsDiagnostic[]>();

function cacheKey(filePath: string, source: string): string {
  const digest = createHash("sha1").update(source).digest("hex");
  return `${filePath}\0${digest}`;
}

function diagnosticsFor(filePath: string, source: string): binding.JsDiagnostic[] {
  const key = cacheKey(filePath, source);
  const cached = fileCache.get(key);
  if (cached) return cached;
  const fresh = binding.lintFile(filePath, source);
  fileCache.set(key, fresh);
  return fresh;
}

// Rules where graphql-eslint reports a single-position `loc` (start only) so
// `endLine`/`endColumn` come back `undefined`. Our underlying diagnostic
// always carries a full range — useful for LSP/CLI consumers — but for these
// specific rules the eslint adapter strips the end so the message shape
// matches graphql-eslint exactly. Add a rule here only when graphql-eslint's
// own implementation is intentionally start-only (e.g. `no-hashtag-description`
// passes `loc: { line, column }` rather than `{ start, end }`).
const START_ONLY_RULES = new Set([
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
      docs: { description },
      schema: OPTIONS_SCHEMA,
      messages: {},
    },
    create(context) {
      return {
        Program() {
          const diagnostics = diagnosticsFor(context.filename, context.sourceCode.text);
          for (const d of diagnostics) {
            if (d.rule !== analyzerRuleName) continue;
            const loc = startOnly
              ? { line: d.line, column: d.column - 1 }
              : {
                  start: { line: d.line, column: d.column - 1 },
                  end: { line: d.endLine, column: d.endColumn - 1 },
                };
            const fix =
              d.fix && ESLINT_FIXABLE_RULES.has(analyzerRuleName)
                ? (fixer: Rule.RuleFixer) => {
                    // Our autofixes carry source positions; ESLint's fixer wants
                    // absolute byte ranges. Compute those from the source text.
                    const text = context.sourceCode.text;
                    const lineStarts = computeLineStarts(text);
                    const edits = d.fix!.edits.map((e) => ({
                      range: [
                        lineStarts[e.rangeStartLine - 1] + (e.rangeStartColumn - 1),
                        lineStarts[e.rangeEndLine - 1] + (e.rangeEndColumn - 1),
                      ] as [number, number],
                      text: e.newText,
                    }));
                    if (edits.length === 1) {
                      return fixer.replaceTextRange(edits[0].range, edits[0].text);
                    }
                    // Multi-edit: chain via the array form.
                    return edits.map((e) => fixer.replaceTextRange(e.range, e.text));
                  }
                : undefined;
            if (d.messageId) {
              ensureMessageId(rule, analyzerRuleName, d.messageId);
              context.report({
                messageId: d.messageId,
                data: { message: d.message },
                loc,
                ...(fix ? { fix } : {}),
              });
            } else {
              context.report({ message: d.message, loc, ...(fix ? { fix } : {}) });
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

export function buildRules(): Record<string, Rule.RuleModule> {
  const rules: Record<string, Rule.RuleModule> = {};
  const meta = binding.getRules();

  for (const rule of meta) {
    const kebabName = toKebabCase(rule.name);
    rules[kebabName] = makeRule(rule.name, rule.description);
  }

  return rules;
}
