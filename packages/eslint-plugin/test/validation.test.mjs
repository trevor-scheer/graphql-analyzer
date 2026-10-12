import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ESLint } from "eslint";
import plugin from "../dist/index.js";

function withProject(run) {
  const root = mkdtempSync(join(tmpdir(), "graphql-validation-"));
  mkdirSync(join(root, "src"));
  writeFileSync(
    join(root, ".graphqlrc.yaml"),
    'schema: "schema.graphql"\ndocuments: "src/**/*.graphql"\n',
  );
  writeFileSync(join(root, "schema.graphql"), "type Query { user: User } type User { id: ID }\n");
  return Promise.resolve(run(root)).finally(() => rmSync(root, { recursive: true, force: true }));
}

function linter(root, parser, rules) {
  return new ESLint({
    cwd: root,
    overrideConfigFile: true,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser },
        plugins: { "@graphql-analyzer": plugin },
        rules,
      },
    ],
  });
}

test("validation rules respect ESLint severity and report suggestions with either parser", () =>
  withProject(async (root) => {
    const filePath = join(root, "src", "op.graphql");
    const source = "fragment F on Usre { id }\n";
    writeFileSync(filePath, source);
    for (const parser of [plugin.parser, plugin.fastParser]) {
      const rules = { "@graphql-analyzer/known-type-names": "warn" };
      const [result] = await linter(root, parser, rules).lintText(source, { filePath });
      const [message] = result.messages.filter(
        (item) => item.ruleId === "@graphql-analyzer/known-type-names",
      );
      assert.equal(message?.severity, 1);
      assert.deepEqual(
        [message.line, message.column, message.endLine, message.endColumn],
        [1, 15, 1, 19],
      );
      assert.ok(message.suggestions?.length);
      assert.equal(message.suggestions[0].fix.text, "User");
      const [disabled] = await linter(root, parser, {
        "@graphql-analyzer/known-type-names": "off",
      }).lintText(source, { filePath });
      assert.equal(disabled.messages.length, 0);
    }
  }));

test("invalid GraphQL does not abort validation with the fast parser", () =>
  withProject(async (root) => {
    const filePath = join(root, "src", "op.graphql");
    const source = "query Q { user { id }\n";
    writeFileSync(filePath, source);
    const [result] = await linter(root, plugin.fastParser, {
      "@graphql-analyzer/known-type-names": "error",
      "@graphql-analyzer/fields-on-correct-type": "error",
    }).lintText(source, { filePath });
    assert.ok(
      result.messages.some(
        (item) => item.message.includes("Expected Name") || item.message.includes('Expected "}"'),
      ),
    );
  }));
