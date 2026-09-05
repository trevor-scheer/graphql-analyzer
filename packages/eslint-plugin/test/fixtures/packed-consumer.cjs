const assert = require("node:assert/strict");
const { createRequire } = require("node:module");
const { resolve } = require("node:path");
const localRequire = createRequire(resolve("package.json"));
const plugin = localRequire("@graphql-analyzer/eslint-plugin");
const { Linter } = localRequire("eslint");
const major = Number(localRequire("eslint/package.json").version.split(".")[0]);
assert.throws(() => localRequire.resolve("@graphql-eslint/eslint-plugin"), /Cannot find module/);
assert.equal(plugin.default.parser, plugin.parser);
assert.equal(plugin.default.fastParser, plugin.fastParser);
assert.equal(plugin.processors.graphql, plugin.processor);
assert.equal(typeof localRequire("@graphql-analyzer/core").lintFile, "function");
const schemaSdl = "type Query { user: User } type User { name: String id: ID! }";
let visited = 0;
const custom = {
  meta: { schema: [], fixable: "code" },
  create(context) {
    const schema = plugin.requireGraphQLSchema("custom/name", context);
    assert.equal(schema.getQueryType().name, "Query");
    return {
      "Field[name.value=name]"(node) {
        visited++;
        assert.equal(node.parent.type, "SelectionSet");
        assert.equal(node.rawNode().kind, "Field");
        assert.equal(node.typeInfo().fieldDef.name, "name");
        context.report({
          node,
          message: "Use id",
          fix: (fixer) => fixer.replaceText(node.name, "id"),
        });
      },
    };
  },
};
const linter = new Linter();
let config;
if (major === 8) {
  linter.defineParser("graphql", plugin.parser);
  linter.defineRule("custom/name", custom);
  linter.defineRule("native/no-anonymous", plugin.rules["no-anonymous-operations"]);
  config = {
    parser: "graphql",
    parserOptions: { schemaSdl },
    rules: { "custom/name": "error", "native/no-anonymous": "error" },
  };
} else {
  config = [
    {
      files: ["**/*.graphql"],
      languageOptions: { parser: plugin.parser, parserOptions: { schemaSdl } },
      plugins: {
        custom: { rules: { name: custom } },
        native: { rules: { "no-anonymous": plugin.rules["no-anonymous-operations"] } },
      },
      rules: { "custom/name": "error", "native/no-anonymous": "error" },
    },
  ];
}
const result = linter.verifyAndFix("query Named { user { name } }", config, {
  filename: resolve("query.graphql"),
});
assert.equal(visited, 1);
assert.equal(result.output, "query Named { user { id } }");
assert.deepEqual(result.messages, []);
const messages = linter.verify("{ user { id } }", config, { filename: resolve("query.graphql") });
assert.equal(messages.length, 1);
assert.equal(messages[0].ruleId, "native/no-anonymous");
const blocks = plugin.processor.preprocess(
  "const q = gql`query Named { user { id } }`;",
  resolve("embedded.js"),
);
assert.equal(blocks.length, 2);
plugin.processor.postprocess(
  blocks.map(() => []),
  resolve("embedded.js"),
);
console.log(`Packed consumer passed: ESLint ${major}, Node ${process.version}`);
