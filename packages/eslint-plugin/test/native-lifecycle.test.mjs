import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

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
  write("schema.graphql", options.schema ?? "type Query { hello: String zebra: String apple: String }");
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

test("native diagnostics use the latest text of preloaded operations", (t) => {
  const p = project(t);
  assert.ok(hasRule(p.lint(), "noAnonymousOperations"));
  assert.equal(hasRule(p.lint("src/op.graphql", "query Named { hello }"), "noAnonymousOperations"), false);
});

test("native schema overlays retain their schema classification", (t) => {
  const p = project(t);
  p.lint();
  assert.ok(hasRule(p.lint("schema.graphql", "# Description\ntype Query { hello: String }"), "noHashtagDescription"));
});

test("native dependencies refresh when schema files change on disk", (t) => {
  const p = project(t, { operation: "query Named { hello }" });
  assert.equal(p.lint().some((d) => d.message.includes("hello") && /not defined|Cannot query|does not exist|unknown/i.test(d.message)), false);
  p.write("schema.graphql", "type Query { other: String }");
  assert.ok(p.lint().some((d) => d.message.includes("hello")), "changed schema must invalidate validation");
});

test("native sibling documents refresh when fragment definitions change", (t) => {
  const p = project(t, { operation: "query Named { ...Fields }" });
  p.write("src/fragment.graphql", "fragment Fields on Query { hello }");
  assert.equal(p.lint().some((d) => /undefined fragment|unknown fragment/i.test(d.message)), false);
  p.write("src/fragment.graphql", "fragment Renamed on Query { hello }");
  assert.ok(p.lint().some((d) => /Fields/.test(d.message)), "changed sibling must invalidate validation");
});

test("unchanged disk contents do not replace unsaved schema overlays", (t) => {
  const p = project(t, { operation: "query Named { added }" });
  p.lint("schema.graphql", "type Query { added: String }");
  assert.equal(p.lint().some((d) => /added/.test(d.message)), false);
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
  const fixed = [...diagnostic.fix.edits].sort((a, b) => b.rangeStartColumn - a.rangeStartColumn).reduce(
    (text, edit) => text.slice(0, edit.rangeStartColumn - 1) + edit.newText + text.slice(edit.rangeEndColumn - 1),
    source,
  );
  assert.equal(fixed, 'query Named($value: String = "🚀") { apple zebra }');
});

test("native embedded diagnostics include the first-line host column", (t) => {
  const p = project(t);
  const source = 'import { gql } from "@apollo/client"; const rocket = "🚀"; const Q = gql`query { hello }`;';
  p.write("src/component.ts", source);
  const diagnostic = p.lint("src/component.ts").find((d) => d.rule === "noAnonymousOperations");
  assert.ok(diagnostic);
  assert.equal(diagnostic.column, source.indexOf("query") + 1);
});
