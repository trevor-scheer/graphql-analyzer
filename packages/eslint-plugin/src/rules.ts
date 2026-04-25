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

function makeRule(analyzerRuleName: string, description: string): Rule.RuleModule {
  return {
    meta: {
      type: "problem",
      docs: { description },
      schema: [],
    },
    create(context) {
      return {
        Program() {
          const diagnostics = diagnosticsFor(context.filename, context.sourceCode.text);
          for (const d of diagnostics) {
            if (d.rule !== analyzerRuleName) continue;
            context.report({
              message: d.message,
              loc: {
                start: { line: d.line, column: d.column - 1 },
                end: { line: d.endLine, column: d.endColumn - 1 },
              },
            });
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
