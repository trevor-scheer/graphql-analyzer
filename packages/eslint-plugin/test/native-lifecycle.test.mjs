import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { buildSchema, introspectionFromSchema } from "graphql";

import { lintFile } from "../dist/binding.js";

function project(t, options = {}) {
  const root = mkdtempSync(path.join(tmpdir(), "graphql-native-lifecycle-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (name, source) => {
    const target = path.join(root, name);
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, source);
    return target;
  };
  const config = {
    schema: "schema.graphql",
    documents: "src/**/*",
    extensions: {
      "graphql-analyzer": {
        lint: {
          rules: {
            noAnonymousOperations: "warn",
            noHashtagDescription: "warn",
            alphabetize: "warn",
          },
        },
      },
    },
  };
  write(
    "schema.graphql",
    options.schema ?? "type Query { hello: String zebra: String apple: String }",
  );
  write("src/op.graphql", options.operation ?? "query { hello }");
  if (options.config !== false) write(".graphqlrc.json", JSON.stringify(config));
  return {
    root,
    write,
    config,
    lint(name = "src/op.graphql", source = readFileSync(path.join(root, name), "utf8")) {
      return lintFile(path.join(root, name), source);
    },
  };
}

const hasRule = (diagnostics, rule) => diagnostics.some((diagnostic) => diagnostic.rule === rule);

function multiProject(t) {
  const p = project(t, { config: false });
  const projects = {};
  for (const name of ["a", "b"]) {
    projects[name] = {
      ...p.config,
      schema: `${name}/schema.graphql`,
      documents: `${name}/src/**/*`,
    };
    p.write(`${name}/schema.graphql`, `type Query { ${name}: String }`);
    p.write(`${name}/src/op.graphql`, `query Named { ${name} }`);
  }
  p.write(".graphqlrc.json", JSON.stringify({ projects }));
  return p;
}

test("native multi-project routing preserves independent schema overlays across A-B-A calls", (t) => {
  const p = multiProject(t);
  p.lint("a/schema.graphql", "type Query { addedA: String }");
  p.lint("b/schema.graphql", "type Query { addedB: String }");
  for (const name of ["a", "b", "a"]) {
    const field = `added${name.toUpperCase()}`;
    assert.equal(
      p
        .lint(`${name}/src/op.graphql`, `query Named { ${field} }`)
        .some((d) => d.message.includes(field)),
      false,
    );
    const other = name === "a" ? "addedB" : "addedA";
    assert.ok(
      p
        .lint(`${name}/src/op.graphql`, `query Named { ${other} }`)
        .some((d) => d.message.includes(other)),
      "each project must validate against its own schema",
    );
  }
  p.write("a/schema.graphql", "type Query { changedA: String }");
  assert.ok(
    p.lint("a/src/op.graphql", "query Named { addedA }").some((d) => d.message.includes("addedA")),
  );
  assert.equal(
    p.lint("b/src/op.graphql", "query Named { addedB }").some((d) => d.message.includes("addedB")),
    false,
    "another project's disk change must preserve the schema overlay",
  );
});

test("native per-call options restore each project's persistent rule configuration", (t) => {
  const p = multiProject(t);
  const source = "query Named { a { value } }";
  p.write("a/schema.graphql", "type Query { a: Item } type Item { value: String }");
  const file = p.write("a/src/op.graphql", source);
  assert.ok(
    hasRule(
      lintFile(file, source, {
        selectionSetDepth: ["error", { maxDepth: 0 }],
      }),
      "selectionSetDepth",
    ),
  );
  assert.equal(
    hasRule(
      lintFile(file, source, {
        selectionSetDepth: ["error", { maxDepth: 1 }],
      }),
      "selectionSetDepth",
    ),
    false,
  );
  assert.equal(hasRule(p.lint("b/src/op.graphql"), "selectionSetDepth"), false);
  assert.equal(hasRule(p.lint("a/src/op.graphql"), "selectionSetDepth"), false);
});

test("native diagnostics use the latest text of preloaded operations", (t) => {
  const p = project(t);
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
  assert.equal(
    hasRule(p.lint("src/op.graphql", "query Named { hello }"), "noAnonymousOperations"),
    false,
  );
});

test("native schema overlays retain their schema classification", (t) => {
  const p = project(t);
  p.lint();
  assert.ok(
    hasRule(
      p.lint("schema.graphql", "# Description\ntype Query { hello: String }"),
      "noHashtagDescription",
    ),
  );
});

test("native dependencies refresh after same-length schema edits on disk", (t) => {
  const p = project(t, {
    schema: "type Query { hello: String }",
    operation: "query Named { hello }",
  });
  assert.equal(
    p
      .lint()
      .some(
        (d) =>
          d.message.includes("hello") &&
          /not defined|Cannot query|does not exist|unknown/i.test(d.message),
      ),
    false,
  );
  p.write("schema.graphql", "type Query { other: String }");
  assert.ok(
    p.lint().some((d) => d.message.includes("hello")),
    "changed schema must invalidate validation",
  );
});

test("native sibling documents refresh when fragment definitions change", (t) => {
  const p = project(t, { operation: "query Named { ...Fields }" });
  p.write("src/fragment.graphql", "fragment Fields on Query { hello }");
  assert.equal(
    p.lint().some((d) => /undefined fragment|unknown fragment/i.test(d.message)),
    false,
  );
  p.write("src/fragment.graphql", "fragment Renamed on Query { hello }");
  assert.ok(
    p.lint().some((d) => /Fields/.test(d.message)),
    "changed sibling must invalidate validation",
  );
});

test("unchanged disk contents do not replace unsaved schema overlays", (t) => {
  const p = project(t, { operation: "query Named { added }" });
  p.lint("schema.graphql", "type Query { added: String }");
  assert.equal(
    p.lint().some((d) => /added/.test(d.message)),
    false,
  );
});

test("native dependency refresh preserves overlays when another file changes", (t) => {
  const p = project(t, { operation: "query Named { added }" });
  p.write("src/fragment.graphql", "fragment Fields on Query { hello }");
  p.lint("schema.graphql", "type Query { hello: String added: String }");
  p.write("src/fragment.graphql", "fragment Fields on Query { alias: hello }");
  assert.equal(
    p.lint().some((d) => /added/.test(d.message)),
    false,
  );
});

test("native sibling dependencies can be deleted and recreated", (t) => {
  const p = project(t, { operation: "query Named { ...Fields }" });
  const fragmentPath = p.write("src/fragment.graphql", "fragment Fields on Query { hello }");
  p.lint();
  rmSync(fragmentPath);
  assert.ok(p.lint().some((d) => /Fields/.test(d.message)));
  p.write("src/fragment.graphql", "fragment Fields on Query { hello }");
  assert.equal(
    p.lint().some((d) => /Fields/.test(d.message)),
    false,
  );
});

test("native introspection dependencies reload without discarding unchanged overlays", (t) => {
  const p = project(t, { operation: "query Named { old }" });
  p.config.schema = ["schema.graphql", "introspection.json"];
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  const introspection = (field) =>
    JSON.stringify(introspectionFromSchema(buildSchema(`type Query { ${field}: String }`)));
  p.write("schema.graphql", "extend type Query { local: String }");
  p.write("introspection.json", introspection("old"));
  p.lint("schema.graphql", "extend type Query { local: String added: String }");
  assert.equal(
    p.lint("src/op.graphql", "query Named { added }").some((d) => /added/.test(d.message)),
    false,
  );
  p.write("introspection.json", introspection("new"));
  assert.equal(
    p.lint("src/op.graphql", "query Named { added }").some((d) => /added/.test(d.message)),
    false,
  );
  assert.ok(p.lint("src/op.graphql", "query Named { old }").some((d) => /old/.test(d.message)));
});

test("native resolved schemas retain precedence after deletion and recreation", (t) => {
  const p = project(t, {
    schema: "type Query { local: String }",
    operation: "query Named { hello }",
  });
  p.config.extensions["graphql-analyzer"].resolvedSchema = "resolved.graphql";
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  const resolvedPath = p.write("resolved.graphql", "type Query { hello: String }");
  assert.equal(
    p.lint().some((d) => /hello/.test(d.message)),
    false,
  );
  rmSync(resolvedPath);
  assert.ok(p.lint().some((d) => /hello/.test(d.message)));
  p.write("resolved.graphql", "type Query { hello: String }");
  assert.equal(
    p.lint().some((d) => /hello/.test(d.message)),
    false,
  );
});

test("native schema reload retries after an unreadable introspection file is repaired", (t) => {
  const p = project(t, { operation: "query Named { hello }" });
  p.config.schema = "introspection.json";
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  const introspection = (field) =>
    JSON.stringify(introspectionFromSchema(buildSchema(`type Query { ${field}: String }`)));
  p.write("introspection.json", introspection("hello"));
  assert.equal(
    p.lint().some((d) => /hello/.test(d.message)),
    false,
  );
  p.write("introspection.json", Buffer.from([0xff]));
  assert.throws(() => p.lint(), /Failed to read schema file/);
  assert.throws(() => p.lint(), /Failed to read schema file/);
  p.write("introspection.json", introspection("other"));
  assert.ok(p.lint().some((d) => /hello/.test(d.message)));
});

test("native configuration can switch from A to B and back to A", (t) => {
  const a = project(t);
  const b = project(t);
  b.config.extensions["graphql-analyzer"].lint.rules.noAnonymousOperations = "off";
  b.write(".graphqlrc.json", JSON.stringify(b.config));
  assert.ok(hasRule(a.lint(), "noAnonymousOperations"));
  assert.equal(hasRule(b.lint(), "noAnonymousOperations"), false);
  assert.ok(hasRule(a.lint(), "noAnonymousOperations"));
});

test("native configuration changes invalidate the active project", (t) => {
  const p = project(t);
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
  p.config.extensions["graphql-analyzer"].lint.rules.noAnonymousOperations = "off";
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  assert.equal(hasRule(p.lint(), "noAnonymousOperations"), false);
});

test("native configuration can be deleted and recreated", (t) => {
  const p = project(t);
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
  rmSync(path.join(p.root, ".graphqlrc.json"));
  assert.equal(hasRule(p.lint(), "noAnonymousOperations"), false);
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
});

test("native configuration discovery retries after a config is created", (t) => {
  const prior = project(t);
  prior.config.extensions["graphql-analyzer"].lint.rules.noAnonymousOperations = "off";
  prior.write(".graphqlrc.json", JSON.stringify(prior.config));
  prior.lint();
  const p = project(t, { config: false });
  p.lint();
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
});

test("native configuration loading retries after an invalid config is fixed", (t) => {
  const p = project(t);
  p.write(".graphqlrc.json", "{ invalid");
  p.lint();
  p.write(".graphqlrc.json", JSON.stringify(p.config));
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
});

test("unconfigured files do not inherit a prior configured project's rules", (t) => {
  const configured = project(t);
  assert.ok(hasRule(configured.lint(), "noAnonymousOperations"));
  const unconfigured = project(t, { config: false });
  assert.equal(hasRule(unconfigured.lint(), "noAnonymousOperations"), false);
});

test("native diagnostic and fix columns use UTF-16 after astral characters", (t) => {
  const source = 'query Named($value: String = "🚀") { zebra apple }';
  const p = project(t, { operation: source });
  const diagnostic = p.lint().find((d) => d.rule === "alphabetize");
  assert.ok(diagnostic?.fix);
  assert.ok(source.slice(diagnostic.column - 1, diagnostic.endColumn - 1).includes("apple"));
  const fixed = [...diagnostic.fix.edits]
    .sort((a, b) => b.rangeStartColumn - a.rangeStartColumn)
    .reduce(
      (text, edit) =>
        text.slice(0, edit.rangeStartColumn - 1) +
        edit.newText +
        text.slice(edit.rangeEndColumn - 1),
      source,
    );
  assert.equal(fixed, 'query Named($value: String = "🚀") { apple zebra }');
});

test("native embedded diagnostics include the first-line host column", (t) => {
  const p = project(t);
  const source =
    'import { gql } from "@apollo/client"; const rocket = "🚀"; const Q = gql`query { hello }`;';
  p.write("src/component.ts", source);
  const diagnostic = p.lint("src/component.ts").find((d) => d.rule === "noAnonymousOperations");
  assert.ok(diagnostic);
  assert.equal(diagnostic.column, source.indexOf("query") + 1);
});

test("native embedded fixes preserve host text and Unicode", (t) => {
  const p = project(t);
  const source =
    'import { gql } from "@apollo/client"; const rocket = "🚀"; const Q = gql`query Named { zebra apple }`;';
  p.write("src/component.ts", source);
  const diagnostic = p.lint("src/component.ts").find((d) => d.rule === "alphabetize");
  assert.ok(diagnostic?.fix);
  const fixed = [...diagnostic.fix.edits]
    .sort((a, b) => b.rangeStartColumn - a.rangeStartColumn)
    .reduce(
      (text, edit) =>
        text.slice(0, edit.rangeStartColumn - 1) +
        edit.newText +
        text.slice(edit.rangeEndColumn - 1),
      source,
    );
  assert.equal(fixed, source.replace("zebra apple", "apple zebra"));
});
