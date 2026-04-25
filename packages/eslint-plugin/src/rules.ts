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

function makeRule(analyzerRuleName: string, description: string): Rule.RuleModule {
  const startOnly = START_ONLY_RULES.has(analyzerRuleName);
  return {
    meta: {
      type: "problem",
      docs: { description },
      schema: OPTIONS_SCHEMA,
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
            context.report({ message: d.message, loc });
          }
        },
      };
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

  return rules;
}
