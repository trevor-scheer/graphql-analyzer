import test from "node:test";
import assert from "node:assert/strict";
import * as path from "node:path";
import * as fs from "node:fs";
import * as os from "node:os";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
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
          suggest: [
            {
              desc: "Use next",
              fix: (fixer) => fixer.replaceText(node.name, "next"),
            },
          ],
        });
      },
    };
  },
};

const filename = fileURLToPath(new URL("./component.js", import.meta.url));
const fixtureRoot = fileURLToPath(
  new URL("../../../test-workspace/eslint-migration/", import.meta.url),
);
const require = createRequire(import.meta.url);

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
        languageOptions: {
          parser: plugin.parser,
          parserOptions: { graphQLConfig: { schema: "" } },
        },
        plugins: { custom: { rules: { rename: rule } } },
        rules: { "custom/rename": "warn" },
      },
    ],
    ...options,
  });
}

test("embedded visitors preserve host linting and map reports, fixes and suggestions", async () => {
  const source = "const q = gql`{ old }`;\ndebugger;\nconst r = /* GraphQL */ `{ old }`;";
  const [result] = await eslint().lintText(source, {
    filePath: filename,
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

test("maps UTF-16 positions through CRLF, indentation and interpolation gaps", async () => {
  const source =
    'const emoji = "😀"; const q = gql`\r\n  query Q {\r\n    ${other}\r\n    old(arg: "😀")\r\n  }\r\n`;';
  const [result] = await eslint().lintText(source, { filePath: filename });
  assert.equal(result.messages.length, 1, JSON.stringify(result.messages));
  const [message] = result.messages;
  assert.equal(message.line, 4);
  assert.equal(message.column, 5);
  assert.equal(message.endLine, 4);
  assert.equal(message.endColumn, 8);
  assert.deepEqual(message.fix.range, [source.indexOf("old("), source.indexOf("old(") + 3]);
  const [inline] = await eslint().lintText('const q = gql`{ field(arg: "😀") old }`;', {
    filePath: filename,
  });
  assert.equal(inline.messages[0].column, 'const q = gql`{ field(arg: "😀") '.length + 1);
});

test("supports aliased gql imports and GraphQL magic comments", async () => {
  const source =
    'import { gql as document } from "@apollo/client";\nconst a = document`{ old }`;\nconst b = /* GraphQL */ `{ old }`;';
  const [result] = await eslint().lintText(source, { filePath: filename });
  assert.equal(result.messages.length, 2);
  assert.ok(result.messages.every((message) => source.slice(...message.fix.range) === "old"));
});

test("resolves project pluck configuration for custom identifiers", async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "graphql-pluck-"));
  try {
    fs.writeFileSync(
      path.join(directory, ".graphqlrc.json"),
      JSON.stringify({
        schema: "",
        extensions: { pluckConfig: { globalGqlIdentifierName: ["document"] } },
      }),
    );
    const source = "const q = document`{ old }`;";
    const [result] = await eslint({ cwd: directory }).lintText(source, {
      filePath: path.join(directory, "component.js"),
    });
    assert.equal(result.messages.length, 1);
    assert.equal(result.messages[0].ruleId, "custom/rename");
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test("autofix and repeated lint passes use fresh embedded source", async () => {
  const linter = eslint({ fix: true });
  const source = "const q = gql`{ ${fragment} old }`;";
  const [fixed] = await linter.lintText(source, { filePath: filename });
  assert.equal(fixed.output, source.replace("old", "next"));
  assert.deepEqual(fixed.messages, []);
  const [repeated] = await linter.lintText(source, { filePath: filename });
  assert.equal(repeated.output, fixed.output);
  assert.deepEqual(repeated.messages, []);
});

test("suppresses edits and suggestions crossing interpolation or escaped backticks", () => {
  for (const source of [
    "const q = gql`{ a ${fragment} old }`;",
    'const q = gql`{ old(arg: "\\`value") }`;',
  ]) {
    const blocks = plugin.processor.preprocess(source, filename);
    const text = blocks[0].text;
    const fix = { range: [0, text.length], text: "{ next }" };
    const [message] = plugin.processor.postprocess(
      [
        [
          {
            ruleId: "custom/unsafe",
            severity: 2,
            message: "unsafe",
            line: 1,
            column: 1,
            fix,
            suggestions: [{ desc: "unsafe", fix }],
          },
        ],
        [],
      ],
      filename,
    );
    assert.equal(message.fix, undefined);
    assert.equal(message.suggestions, undefined);
  }
});

test("does not insert JavaScript delimiters through GraphQL fixes", () => {
  for (const replacement of ["`", "${code}", "\\n"]) {
    plugin.processor.preprocess("const q = gql`{ old }`;", filename);
    const [message] = plugin.processor.postprocess(
      [
        [
          {
            ruleId: "custom/unsafe",
            severity: 2,
            message: "unsafe",
            line: 1,
            column: 3,
            fix: { range: [2, 5], text: replacement },
          },
        ],
        [],
      ],
      filename,
    );
    assert.equal(message.fix, undefined);
  }
});

test("suppresses fixes that create interpolation across an edit boundary", () => {
  for (const [source, fix] of [
    ["const q = gql`{ old }`;", { range: [0, 0], text: "$" }],
    ["const q = gql`query Q($var: ID) { old }`;", { range: [9, 12], text: "{" }],
  ]) {
    plugin.processor.preprocess(source, filename);
    const [message] = plugin.processor.postprocess(
      [[{ ruleId: "custom/unsafe", severity: 2, message: "unsafe", line: 1, column: 1, fix }], []],
      filename,
    );
    assert.equal(message.fix, undefined);
  }
});

test("preserves raw GraphQL escapes while mapping after escaped backticks", async () => {
  const source = 'const q = gql`{ field(arg: "\\`value\\n") old }`;';
  const [result] = await eslint().lintText(source, { filePath: filename });
  assert.equal(result.messages.length, 1, JSON.stringify(result.messages));
  assert.equal(result.messages[0].ruleId, "custom/rename");
  assert.deepEqual(result.messages[0].fix.range, [
    source.indexOf("old"),
    source.indexOf("old") + 3,
  ]);
});

test("host syntax errors survive preprocessing and do not retain prior blocks", async () => {
  const linter = eslint();
  await linter.lintText("const q = gql`{ old }`;", { filePath: filename });
  const [result] = await linter.lintText("const = gql`{ old }`;", {
    filePath: filename,
  });
  assert.equal(result.messages.length, 1);
  assert.equal(result.messages[0].fatal, true);
  assert.equal(result.messages[0].ruleId, null);
});

test("maps GraphQL parse errors back to the host literal", async () => {
  const source = "const q = gql`{ old( }`;";
  const [result] = await eslint().lintText(source, { filePath: filename });
  assert.equal(result.messages.length, 1);
  assert.equal(result.messages[0].fatal, true);
  assert.equal(result.messages[0].column, source.indexOf("}") + 1);
});

test("native and custom GraphQL rules coexist with host rules without duplicate reports", async () => {
  const fixture = fixtureRoot;
  const linter = eslint({
    cwd: fixture,
    overrideConfig: [
      {
        files: ["**/*.js"],
        processor: plugin.processor,
        plugins: { native: plugin },
        rules: {
          "native/no-anonymous-operations": "error",
          "no-debugger": "error",
        },
      },
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { native: plugin, custom: { rules: { rename: rule } } },
        rules: {
          "native/no-anonymous-operations": "error",
          "custom/rename": "warn",
        },
      },
    ],
  });
  const source =
    'import { gql } from "@apollo/client";\nconst q = gql`{ old }`;\nconst r = gql`{ old }`;\ndebugger;';
  const [result] = await linter.lintText(source, {
    filePath: path.join(fixture, "src/embedded.js"),
  });
  assert.equal(
    result.messages.filter((message) => message.ruleId === "native/no-anonymous-operations").length,
    2,
    JSON.stringify(result.messages),
  );
  assert.equal(result.messages.filter((message) => message.ruleId === "custom/rename").length, 2);
  assert.equal(result.messages.filter((message) => message.ruleId === "no-debugger").length, 1);
  const native = result.messages.filter(
    (message) => message.ruleId === "native/no-anonymous-operations",
  );
  assert.equal(native[0].column, "const q = gql`".length + 1);
});

test("fast processor preserves original-source native processing", () => {
  const source = "const q = gql`{ old }`;";
  assert.deepEqual(plugin.fastProcessor.preprocess(source, "component.js"), [source]);
});

test("compatible processor preserves standalone GraphQL source", () => {
  const source = "query Named { old }";
  for (const extension of ["graphql", "gql"]) {
    assert.deepEqual(plugin.processor.preprocess(source, `document.${extension}`), [source]);
  }
});

test("native suggestions map to embedded UTF-16 host ranges", async () => {
  const source =
    'import { gql } from "@apollo/client";\nconst emoji = "😀"; const q = gql`query Named { user(id: "1") { id id } }`;';
  const linter = new ESLint({
    cwd: fixtureRoot,
    overrideConfigFile: true,
    overrideConfig: [
      { files: ["**/*.js"], processor: plugin.processor },
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { native: plugin },
        rules: { "native/no-duplicate-fields": "error" },
      },
    ],
  });
  const physical = path.join(fixtureRoot, "src/suggestion.js");
  const [result] = await linter.lintText(source, { filePath: physical });
  assert.equal(result.messages.length, 1, JSON.stringify(result.messages));
  const fix = result.messages[0].suggestions[0].fix;
  assert.equal(source.slice(...fix.range).trim(), "id");
  const revised = source.slice(0, fix.range[0]) + fix.text + source.slice(fix.range[1]);
  const [fixed] = await linter.lintText(revised, { filePath: physical });
  assert.deepEqual(fixed.messages, []);
});

test("native ignore directives work in compatible and fast embedded modes", async () => {
  for (const processor of [plugin.processor, plugin.fastProcessor]) {
    const linter = new ESLint({
      cwd: fixtureRoot,
      overrideConfigFile: true,
      overrideConfig: [
        { files: ["**/*.js"], processor },
        { files: ["**/*.graphql"], languageOptions: { parser: plugin.parser } },
        {
          files: ["**/*.{js,graphql}"],
          plugins: { "@graphql-analyzer": plugin },
          rules: { "@graphql-analyzer/no-anonymous-operations": "error" },
        },
      ],
    });
    for (const directive of [
      "eslint-disable-next-line @graphql-analyzer/no-anonymous-operations",
      "graphql-analyzer-ignore: noAnonymousOperations",
    ]) {
      const source = `import { gql } from "@apollo/client";\nconst q = gql\`\n# ${directive}\n{ __typename }\n\`;`;
      const [result] = await linter.lintText(source, {
        filePath: path.join(fixtureRoot, "src/ignored.js"),
      });
      assert.deepEqual(result.messages, [], `${processor.meta.name}: ${directive}`);
    }
  }
});

test("compatible native directives are applied once and retain unused-directive warnings", async () => {
  const linter = new ESLint({
    cwd: fixtureRoot,
    overrideConfigFile: true,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { "@graphql-analyzer": plugin },
        rules: { "@graphql-analyzer/no-anonymous-operations": "error" },
      },
    ],
  });
  const physical = path.join(fixtureRoot, "src/directives.graphql");
  for (const rule of ["", " @graphql-analyzer/no-anonymous-operations"]) {
    const [suppressed] = await linter.lintText(
      `# eslint-disable-next-line${rule}\n{ __typename }`,
      { filePath: physical },
    );
    assert.deepEqual(suppressed.messages, []);
    const [unused] = await linter.lintText(
      `# eslint-disable-next-line${rule}\nquery Named { __typename }`,
      { filePath: physical },
    );
    assert.equal(unused.messages.length, 1);
    assert.match(unused.messages[0].message, /Unused eslint-disable directive/);
    const [enabled] = await linter.lintText(
      `# eslint-disable${rule}\n{ __typename }\n# eslint-enable${rule}\n{ __typename }`,
      { filePath: physical },
    );
    assert.equal(enabled.messages.length, 1, JSON.stringify(enabled.messages));
    assert.equal(enabled.messages[0].ruleId, "@graphql-analyzer/no-anonymous-operations");
    assert.equal(enabled.messages[0].line, 4);
  }
});

test("embedded directive ownership stays isolated across blocks and the host", async () => {
  const linter = new ESLint({
    cwd: fixtureRoot,
    overrideConfigFile: true,
    overrideConfig: [
      { files: ["**/*.js"], processor: plugin.processor, rules: { "no-debugger": "error" } },
      { files: ["**/*.graphql"], languageOptions: { parser: plugin.parser } },
      {
        files: ["**/*.{js,graphql}"],
        plugins: { "@graphql-analyzer": plugin },
        rules: { "@graphql-analyzer/no-anonymous-operations": "error" },
      },
    ],
  });
  const source =
    'import { gql } from "@apollo/client";\n' +
    "const a = gql`{ __typename }`;\n" +
    "const b = gql`\n# eslint-disable-next-line\n{ __typename }\n`;\n" +
    "debugger;";
  const [result] = await linter.lintText(source, {
    filePath: path.join(fixtureRoot, "src/mixed-directives.js"),
  });
  assert.deepEqual(
    result.messages.map(({ ruleId, line }) => ({ ruleId, line })),
    [
      { ruleId: "@graphql-analyzer/no-anonymous-operations", line: 2 },
      { ruleId: "no-debugger", line: 7 },
    ],
  );
});

test("disabling inline config preserves compatible native reports", async () => {
  const linter = new ESLint({
    cwd: fixtureRoot,
    overrideConfigFile: true,
    allowInlineConfig: false,
    overrideConfig: [
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { "@graphql-analyzer": plugin },
        rules: { "@graphql-analyzer/no-anonymous-operations": "error" },
      },
    ],
  });
  const [result] = await linter.lintText("# eslint-disable-next-line\n{ __typename }", {
    filePath: path.join(fixtureRoot, "src/no-inline.graphql"),
  });
  assert.equal(result.messages.length, 1);
  assert.equal(result.messages[0].ruleId, "@graphql-analyzer/no-anonymous-operations");
});

test("uses one physical native analysis across blocks and host rules", async () => {
  const binding = require("../dist/binding.js");
  const original = binding.lintFile;
  const calls = [];
  binding.lintFile = (physical, source, overrides) => {
    calls.push(physical);
    return original(physical, source, overrides);
  };
  try {
    const fixture = fixtureRoot;
    const physical = path.join(fixture, "src/cached-embedded.js");
    const linter = new ESLint({
      cwd: fixture,
      overrideConfigFile: true,
      overrideConfig: [
        { files: ["**/*.js"], processor: plugin.processor },
        { files: ["**/*.graphql"], languageOptions: { parser: plugin.parser } },
        {
          files: ["**/*.{js,graphql}"],
          plugins: { native: plugin },
          rules: {
            "native/no-anonymous-operations": "error",
            "native/no-duplicate-fields": "error",
          },
        },
      ],
    });
    await linter.lintText(
      'import { gql } from "@apollo/client"; const a = gql`{ __typename }`; const b = gql`{ __typename }`;',
      { filePath: physical },
    );
    assert.deepEqual(calls, [physical]);
  } finally {
    binding.lintFile = original;
  }
});

test("embedded native cache separates virtual rule options and subsequent lint passes", async () => {
  const binding = require("../dist/binding.js");
  const original = binding.lintFile;
  const calls = [];
  binding.lintFile = (physical, source, overrides) => {
    calls.push({ physical, overrides });
    return [];
  };
  try {
    const physical = path.join(fixtureRoot, "src/options.js");
    const source = "const a = gql`{ __typename }`; const b = gql`{ __typename }`;";
    const configs = [
      { files: ["**/*.js"], processor: plugin.processor },
      {
        files: ["**/*.graphql"],
        languageOptions: { parser: plugin.parser },
        plugins: { native: plugin },
        rules: { "native/no-anonymous-operations": "error" },
      },
    ];
    const linter = new ESLint({
      cwd: fixtureRoot,
      overrideConfigFile: true,
      overrideConfig: [
        ...configs,
        {
          files: ["**/0_document.graphql"],
          rules: { "native/alphabetize": ["error", { selections: [] }] },
        },
        {
          files: ["**/1_document.graphql"],
          rules: {
            "native/alphabetize": ["error", { selections: ["OperationDefinition"] }],
          },
        },
      ],
    });
    for (let pass = 0; pass < 2; pass++) {
      const [result] = await linter.lintText(source, { filePath: physical });
      assert.deepEqual(result.messages, []);
      assert.equal(calls.length, (pass + 1) * 2);
      assert.deepEqual(calls[pass * 2].overrides.alphabetize.options, { selections: [] });
      assert.deepEqual(calls[pass * 2 + 1].overrides.alphabetize.options, {
        selections: ["OperationDefinition"],
      });
    }
    await new ESLint({
      cwd: fixtureRoot,
      overrideConfigFile: true,
      overrideConfig: configs,
    }).lintText(source, { filePath: physical });
    assert.equal(calls.length, 5);
    assert.deepEqual(calls[4].overrides, { noAnonymousOperations: { severity: "warn" } });
    assert.ok(calls.every((call) => call.physical === physical));
  } finally {
    binding.lintFile = original;
  }
});

for (const [extension, source] of Object.entries({
  vue: '<script setup>\nimport { gql } from "@apollo/client";\nconst q = gql`{ old }`;\n</script>\n<template><div /></template>',
  svelte:
    '<script>\nimport { gql } from "@apollo/client";\nconst q = gql`{ old }`;\n</script>\n<div/>',
  astro: '---\nimport { gql } from "@apollo/client";\nconst q = gql`{ old }`;\n---\n<div/>',
})) {
  test(`maps unique static GraphQL template reports and fixes in ${extension}`, () => {
    const physical = filename.replace(/js$/, extension);
    const blocks = plugin.processor.preprocess(source, physical);
    assert.equal(blocks.length, 2);
    assert.equal(blocks[0].text, "{ old }");
    assert.equal(blocks[1], source);
    const [message] = plugin.processor.postprocess(
      [
        [
          {
            ruleId: "custom/rename",
            severity: 2,
            message: "rename",
            line: 1,
            column: 3,
            endLine: 1,
            endColumn: 6,
            fix: { range: [2, 5], text: "next" },
          },
        ],
        [],
      ],
      physical,
    );
    assert.equal(message.line, 3);
    assert.equal(message.column, 17);
    assert.equal(message.endColumn, 20);
    assert.deepEqual(message.fix.range, [source.indexOf("old"), source.indexOf("old") + 3]);
  });
}

test("rejects ambiguous transformed component documents with an actionable error", () => {
  const physical = filename.replace(/js$/, "svelte");
  const source = "<script>const a = gql`{ old }`; const b = gql`{ old }`;</script>";
  assert.throws(
    () => plugin.processor.preprocess(source, physical),
    /Cannot uniquely map.*Extract the document to a .graphql file/,
  );
  assert.deepEqual(plugin.processor.postprocess([[]], physical), []);
});

test("releases processor records if a rule aborts ESLint before postprocess", async () => {
  const { getEmbeddedRecord } = require("../dist/embedded.js");
  plugin.processor.preprocess("const q = gql`{ old }`;", filename);
  assert.ok(getEmbeddedRecord(filename));
  await Promise.resolve();
  assert.equal(getEmbeddedRecord(filename), undefined);
});

test("sibling services overlay every unsaved embedded block without duplicate fragments", async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "graphql-siblings-"));
  try {
    const physical = path.join(directory, "component.js");
    fs.writeFileSync(path.join(directory, "schema.graphql"), "type Query { old: String }");
    fs.writeFileSync(physical, "const f = gql`fragment Disk on Query { old }`;");
    fs.writeFileSync(
      path.join(directory, ".graphqlrc.json"),
      JSON.stringify({ schema: "schema.graphql", documents: "*.js" }),
    );
    const visits = [];
    const inspect = {
      meta: { schema: [] },
      create(context) {
        return {
          Document() {
            const siblings = plugin.requireGraphQLOperations("custom/siblings", context);
            visits.push({
              fragments: siblings.getFragments().map(({ document }) => document.name.value),
              operations: siblings.getOperations().map(({ document }) => document.name.value),
            });
          },
        };
      },
    };
    const linter = new ESLint({
      cwd: directory,
      overrideConfigFile: true,
      overrideConfig: [
        { files: ["**/*.js"], processor: plugin.processor },
        {
          files: ["**/*.graphql"],
          languageOptions: { parser: plugin.parser },
          plugins: { custom: { rules: { siblings: inspect } } },
          rules: { "custom/siblings": "error" },
        },
      ],
    });
    const [result] = await linter.lintText(
      "const f = gql`fragment Fresh on Query { old }`; const q = gql`query Read { ...Fresh }`;",
      { filePath: physical },
    );
    assert.deepEqual(result.messages, []);
    assert.deepEqual(visits, [
      { fragments: ["Fresh"], operations: ["Read"] },
      { fragments: ["Fresh"], operations: ["Read"] },
    ]);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test("concurrent ESLint instances map different sources for the same physical filename", async () => {
  const sources = [
    "const q = gql`{ old }`;",
    'const prefix = "😀";\nconst q = gql`{ ${fragment} old }`; const r = gql`{ old }`;',
  ];
  const results = await Promise.all(
    sources.map((source) => eslint({ fix: true }).lintText(source, { filePath: filename })),
  );
  for (const [index, [result]] of results.entries()) {
    assert.deepEqual(result.messages, []);
    assert.equal(result.output, sources[index].replaceAll("old", "next"));
  }
});
