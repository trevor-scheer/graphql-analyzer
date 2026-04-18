import type { Rule } from "eslint";
import * as binding from "./binding";

function toKebabCase(name: string): string {
  return name.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
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
          const diagnostics = binding.lintFile(
            context.filename,
            context.sourceCode.text,
          );
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
