import assert from "node:assert/strict";
import { copyFileSync, mkdtempSync, writeFileSync, mkdirSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { Linter, RuleTester } from "eslint";
import { Linter as UpstreamLinter } from "eslint-v9";
import { buildSchema, introspectionFromSchema, print, validate } from "graphql";
import upstream from "@graphql-eslint/eslint-plugin";
const require = createRequire(import.meta.url);
const plugin = require("../dist");
const schemaSdl = "type Query { user: User } type User { name: String id: ID! }";

function lint(parser, code, rule, parserOptions = { schemaSdl }, filePath = "/tmp/custom.graphql") {
  const CompatibleLinter = parser === upstream.parser ? UpstreamLinter : Linter;
  return new CompatibleLinter({ cwd: "/tmp" }).verify(
    code,
    [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser, parserOptions },
        plugins: { custom: { rules: { example: rule } } },
        rules: { "custom/example": "error" },
      },
    ],
    { filename: filePath },
  );
}

function fixture(t, files) {
  const directory = mkdtempSync(join(tmpdir(), "graphql-custom-rules-"));
  for (const [name, source] of Object.entries(files)) {
    const filePath = join(directory, name);
    mkdirSync(resolve(filePath, ".."), { recursive: true });
    writeFileSync(filePath, source);
  }
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

test("custom selectors and exit listeners run with parent pointers and source text", () => {
  function observe(parser) {
    const visited = [];
    const messages = lint(parser, "query Named { user { name } }", {
      meta: { schema: [] },
      create(context) {
        return {
          OperationDefinition(node) {
            visited.push(["enter", node.name.value]);
          },
          "Field[name.value=name]"(node) {
            visited.push([node.type, node.parent.type, context.sourceCode.getText(node)]);
            context.report({ node, message: "Use id" });
          },
          "OperationDefinition:exit"(node) {
            visited.push(["exit", node.name.value]);
          },
        };
      },
    });
    return {
      visited,
      messages: messages.map(({ message, line, column }) => ({ message, line, column })),
    };
  }
  assert.deepEqual(observe(plugin.parser), observe(upstream.parser));
  assert.equal(observe(plugin.parser).messages.length, 1);
});

test("ordinary RuleTester supports reports, suggestions, and autofixes", () => {
  const rule = {
    meta: {
      schema: [],
      fixable: "code",
      hasSuggestions: true,
      messages: { rename: "Rename field", suggest: "Use id" },
    },
    create(context) {
      return {
        "Field[name.value=name]"(node) {
          context.report({
            node: node.name,
            messageId: "rename",
            fix: (fixer) => fixer.replaceText(node.name, "id"),
            suggest: [{ messageId: "suggest", fix: (fixer) => fixer.replaceText(node.name, "id") }],
          });
        },
      };
    },
  };
  new RuleTester({ languageOptions: { parser: plugin.parser, parserOptions: { schemaSdl } } }).run(
    "rename",
    rule,
    {
      valid: ["{ user { id } }"],
      invalid: [
        {
          code: "{ user { name } }",
          output: "{ user { id } }",
          errors: [
            {
              messageId: "rename",
              suggestions: [{ messageId: "suggest", output: "{ user { id } }" }],
            },
          ],
        },
      ],
    },
  );
});

test("empty and comment-only documents remain valid and preserve comments", () => {
  for (const code of ["", "# just a comment\n"]) {
    const actual = plugin.parseForESLint(code, { schemaSdl });
    assert.deepEqual(actual.ast.range, [0, code.length]);
    assert.equal(actual.ast.comments.length, code ? 1 : 0);
    assert.equal(actual.ast.body[0].rawNode().definitions.length, 0);
  }
});

test("public helpers require configured schema and operations", () => {
  const parsed = plugin.parseForESLint("{ user { id } }", { graphQLConfig: { schema: "" } });
  const context = { sourceCode: { parserServices: parsed.services } };
  assert.equal(parsed.services.schema, null);
  assert.equal(parsed.services.siblingOperations.available, false);
  assert.throws(
    () => plugin.requireGraphQLSchema("custom/rule", context),
    /requires graphql-config `schema`/,
  );
  assert.throws(
    () => plugin.requireGraphQLOperations("custom/rule", context),
    /requires graphql-config `documents`/,
  );
});

test("syntax errors have ESLint coordinates and invalid configured schemas fail parsing", () => {
  const errors = lint(plugin.parser, "{\n user( }", { meta: { schema: [] }, create: () => ({}) });
  assert.equal(errors[0].fatal, true);
  assert.equal(errors[0].line, 2);
  assert.equal(errors[0].column, 8);
  assert.throws(
    () => plugin.parseForESLint("{ name }", { schemaSdl: "type Query {" }),
    /Syntax Error/,
  );
  assert.throws(
    () => plugin.parseForESLint("{ name }", { schema: "type Query { name: String }" }),
    /removed in graphql-eslint@4/,
  );
});

test("inline services provide graphql-js schemas and recursive sibling fragments", () => {
  const options = {
    filePath: "/tmp/new.graphql",
    graphQLConfig: {
      schema: schemaSdl,
      documents: "fragment A on User { ...B } fragment B on User { id }",
    },
  };
  const parsed = plugin.parseForESLint("query Current { user { ...A } }", options);
  const context = { sourceCode: { parserServices: parsed.services } };
  const schema = plugin.requireGraphQLSchema("custom/example", context);
  const siblings = plugin.requireGraphQLOperations("custom/example", context);
  assert.equal(schema.getQueryType().name, "Query");
  assert.deepEqual(
    siblings.getFragmentByType("User").map((source) => source.document.name.value),
    ["A", "B"],
  );
  assert.deepEqual(
    siblings
      .getFragmentsInUse(parsed.ast.body[0].rawNode().definitions[0])
      .map((node) => node.name.value),
    ["A", "B"],
  );
  assert.equal(siblings.getOperations()[0].document.name.value, "Current");
  assert.equal(siblings.getOperationByType("query").length, 1);
  assert.equal(siblings.getOperation("Current").length, 1);
});

test("disk services refresh schema, fragments, and config while reusing unchanged schemas", (t) => {
  const directory = fixture(t, {
    "graphql.config.json": JSON.stringify({
      schema: "schema.graphql",
      documents: "operations/*.graphql",
    }),
    "schema.graphql": schemaSdl,
    "operations/query.graphql": "query Saved { user { id } }",
    "operations/fragment.graphql": "fragment Saved on User { id }",
  });
  const filePath = join(directory, "operations/query.graphql");
  const parse = () => plugin.parseForESLint("query Unsaved { user { name } }", { filePath });
  const first = parse();
  assert.equal(first.services.siblingOperations.getOperation("Saved").length, 0);
  assert.equal(first.services.siblingOperations.getOperation("Unsaved").length, 1);
  assert.equal(parse().services.schema, first.services.schema);
  writeFileSync(
    join(directory, "schema.graphql"),
    "type Query { user: User } type User { name: String id: ID! age: Int }",
  );
  const second = parse();
  assert.notEqual(second.services.schema, first.services.schema);
  assert(second.services.schema.getType("User").getFields().age);
  writeFileSync(join(directory, "operations/fragment.graphql"), "fragment Edited on User { name }");
  assert.equal(parse().services.siblingOperations.getFragment("Edited").length, 1);
  writeFileSync(
    join(directory, "graphql.config.json"),
    JSON.stringify({ schema: "type Query { replacement: Boolean }" }),
  );
  assert(parse().services.schema.getQueryType().getFields().replacement);
});

test("schema overlays replace unsaved SDL before merging extensions", (t) => {
  const directory = fixture(t, {
    "graphql.config.json": JSON.stringify({ schema: ["schema.graphql", "extension.graphql"] }),
    "schema.graphql": "type Query { old: String }",
    "extension.graphql": "extend type Query { extension: Int }",
  });
  const parsed = plugin.parseForESLint("type Query { edited: ID }", {
    filePath: join(directory, "schema.graphql"),
  });
  assert.deepEqual(Object.keys(parsed.services.schema.getQueryType().getFields()).sort(), [
    "edited",
    "extension",
  ]);
  assert.deepEqual(
    validate(
      parsed.services.schema,
      plugin
        .parseForESLint("{ edited extension }", { schemaSdl: print(parsed.ast.body[0].rawNode()) })
        .ast.body[0].rawNode(),
    ),
    [],
  );
});

test("multi-project selection uses the complete path for nonexistent documents", (t) => {
  const directory = fixture(t, {
    "graphql.config.json": JSON.stringify({
      projects: {
        first: { schema: "type Query { first: String }", documents: "first/*.graphql" },
        second: { schema: "type Query { second: String }", documents: "second/*.graphql" },
      },
    }),
    "first/existing.graphql": "{ first }",
    "second/existing.graphql": "{ second }",
  });
  for (const name of ["first", "second", "first"]) {
    const parsed = plugin.parseForESLint(`{ ${name} }`, {
      filePath: join(directory, name, "unsaved.graphql"),
    });
    assert(parsed.services.schema.getQueryType().getFields()[name]);
  }
});

test("federation linked schemas provide genuine subgraph schema objects", () => {
  const schema = `extend schema @link(url: "https://specs.apollo.dev/federation/v2.3", import: ["@key"])
    type Query { product: Product } type Product @key(fields: "id") { id: ID! }`;
  const parsed = plugin.parseForESLint("{ product { id } }", { graphQLConfig: { schema } });
  assert(parsed.services.schema.getQueryType().getFields()._service);
});

test("schema glob additions and deletions invalidate cached schemas", (t) => {
  const directory = fixture(t, {
    "graphql.config.json": JSON.stringify({ schema: "schema/*.graphql" }),
    "schema/query.graphql": "type Query { name: String }",
  });
  const parse = () =>
    plugin.parseForESLint("{ name }", { filePath: join(directory, "operation.graphql") });
  assert.equal(parse().services.schema.getQueryType().getFields().extra, undefined);
  writeFileSync(join(directory, "schema/extra.graphql"), "extend type Query { extra: Int }");
  assert(parse().services.schema.getQueryType().getFields().extra);
  rmSync(join(directory, "schema/extra.graphql"));
  assert.equal(parse().services.schema.getQueryType().getFields().extra, undefined);
});

test("introspection JSON pointers build compatible schemas", (t) => {
  const directory = fixture(t, {
    "graphql.config.json": JSON.stringify({ schema: "introspection.json" }),
    "introspection.json": JSON.stringify({ data: introspectionFromSchema(buildSchema(schemaSdl)) }),
  });
  const parsed = plugin.parseForESLint("{ user { id } }", {
    filePath: join(directory, "query.graphql"),
  });
  assert.equal(parsed.services.schema.getType("User").getFields().id.type.toString(), "ID!");
});

test("CommonJS config edits are observed on the next parse", (t) => {
  const directory = fixture(t, {
    "graphql.config.cjs": "module.exports = {schema: 'type Query { first: String }'};",
  });
  const parse = () =>
    plugin.parseForESLint("{ first }", { filePath: join(directory, "query.graphql") });
  assert(parse().services.schema.getQueryType().getFields().first);
  writeFileSync(
    join(directory, "graphql.config.cjs"),
    "module.exports = {schema: 'type Query { second: Int }'};",
  );
  assert(parse().services.schema.getQueryType().getFields().second);
});

test("root import and fast entry points do not load compatibility dependencies", () => {
  const script = `const Module = require('node:module'); const loaded = []; const original = Module._load;
    Module._load = function(name, ...args) { loaded.push(name); return original.call(this, name, ...args); };
    const plugin = require(${JSON.stringify(require.resolve("../dist"))});
    plugin.fastParser.parseForESLint('{ user { id } }'); plugin.fastProcessor.preprocess('query Named { name }', '/tmp/query.graphql');
    const heavy = loaded.filter(name => name === 'graphql' || name.startsWith('graphql-config') || name.startsWith('@graphql-tools/') || name === '@apollo/subgraph');
    if (heavy.length) throw new Error(JSON.stringify(heavy));`;
  execFileSync(process.execPath, ["-e", script]);
});

test("public custom-rule types compile in a consumer with ordinary RuleTester", (t) => {
  const directory = mkdtempSync(new URL("./consumer-", import.meta.url));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  mkdirSync(join(directory, "node_modules/@graphql-analyzer"), { recursive: true });
  symlinkSync(
    fileURLToPath(new URL("..", import.meta.url)),
    join(directory, "node_modules/@graphql-analyzer/eslint-plugin"),
    "junction",
  );
  const filePath = join(directory, "custom-rule.ts");
  copyFileSync(new URL("./fixtures/custom-rule.ts", import.meta.url), filePath);
  execFileSync(
    process.execPath,
    [
      resolve(require.resolve("typescript-compiler/package.json"), "../bin/tsc"),
      "--ignoreConfig",
      "--noEmit",
      "--strict",
      "--skipLibCheck",
      "--target",
      "ES2022",
      "--module",
      "nodenext",
      "--moduleResolution",
      "nodenext",
      filePath,
    ],
    { stdio: "pipe" },
  );
});
