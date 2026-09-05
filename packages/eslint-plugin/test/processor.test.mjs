import test from "node:test";
import assert from "node:assert/strict";
import * as path from "node:path";
import { ESLint } from "eslint";
import plugin from "../dist/index.js";

const rule = {
  meta: { type: "problem", fixable: "code", hasSuggestions: true, schema: [] },
  create(context) {
    return {
      Field(node) {
        if (node.name.value !== "old") return;
        context.report({
          node: node.name,
          message: "Rename old",
          fix: (fixer) => fixer.replaceText(node.name, "next"),
          suggest: [{ desc: "Use next", fix: (fixer) => fixer.replaceText(node.name, "next") }],
        });
      },
    };
  },
};

function eslint(options = {}) {
  return new ESLint({
    overrideConfigFile: true,
    overrideConfig: [
      {
        files: ["**/*.js"],
        processor: plugin.processor,
        rules: { "no-debugger": "error" },
      },
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser, parserOptions: { graphQLConfig: false } },
        plugins: { custom: { rules: { rename: rule } } },
        rules: { "custom/rename": "warn" },
      },
    ],
    ...options,
  });
}

test("embedded visitors preserve host linting and map reports, fixes and suggestions", async () => {
  const source = 'const q = gql`{ old }`;\ndebugger;\nconst r = /* GraphQL */ `{ old }`;';
  const [result] = await eslint().lintText(source, {
    filePath: path.resolve("packages/eslint-plugin/test/component.js"),
  });
  assert.equal(result.messages.length, 3);
  const embedded = result.messages.filter((message) => message.ruleId === "custom/rename");
  assert.equal(embedded.length, 2);
  for (const message of embedded) {
    assert.equal(message.severity, 1);
    assert.equal(source.slice(...message.fix.range), "old");
    assert.deepEqual(message.suggestions[0].fix, message.fix);
    assert.equal(message.endColumn - message.column, 3);
  }
  assert.equal(embedded[0].column, source.indexOf("old") + 1);
  assert.equal(result.messages.find((message) => message.ruleId === "no-debugger").line, 2);
});

test("fast processor preserves original-source native processing", () => {
  const source = "const q = gql`{ old }`;";
  assert.deepEqual(plugin.fastProcessor.preprocess(source, "component.js"), [source]);
});
