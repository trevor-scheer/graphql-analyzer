import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync, statSync, utimesSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRequire } from "node:module";
import { test } from "node:test";
const require = createRequire(import.meta.url);
const { loadSources } = require("../dist/source-cache.js");

function fixture(t, name, text) {
  const directory = mkdtempSync(join(tmpdir(), "graphql-source-cache-"));
  const filePath = join(directory, name);
  writeFileSync(filePath, text);
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return {
    filePath,
    project: {
      filepath: join(directory, "graphql.config.json"),
      dirpath: directory,
      name: "default",
      schema: name,
      extensions: {},
    },
  };
}

test("unchanged files reuse loaded sources; changes invalidate even with restored mtime", (t) => {
  const { filePath, project } = fixture(t, "schema.graphql", "type Query { first: Int }");
  let loads = 0;
  const read = () =>
    loadSources(project, "schema", () => {
      loads++;
      return [];
    });
  const first = read();
  assert.equal(read(), first);
  assert.equal(loads, 1);
  const before = statSync(filePath);
  writeFileSync(filePath, "type Query { other: Int }");
  utimesSync(filePath, before.atime, before.mtime);
  assert.notEqual(read(), first);
  assert.equal(loads, 2);
});

test("importing GraphQL files and executable schema sources bypass snapshots", (t) => {
  for (const [name, text] of [
    ["schema.graphql", '# import Query from "./other.graphql"'],
    ["schema.js", "module.exports = 'type Query { name: String }'"],
  ]) {
    const { project } = fixture(t, name, text);
    let loads = 0;
    const read = () =>
      loadSources(project, "schema", () => {
        loads++;
        return [];
      });
    assert.equal(read().key, undefined);
    read();
    assert.equal(loads, 2);
  }
});

test("remote pointers, custom loaders, and dynamic options bypass snapshots", () => {
  for (const config of [
    { schema: "https://example.test/graphql" },
    { schema: "github:owner/repo#main:schema.graphql" },
    { schema: { "schema.graphql": { loader: "custom" } } },
    {
      schema: "type Query { name: String }",
      extensions: { pluckConfig: { isGqlTemplateLiteral: () => true } },
    },
  ]) {
    const project = {
      filepath: "/tmp/graphql.config.json",
      dirpath: "/tmp",
      name: "default",
      extensions: {},
      ...config,
    };
    let loads = 0;
    const read = () =>
      loadSources(project, "schema", () => {
        loads++;
        return [];
      });
    assert.equal(read().key, undefined);
    read();
    assert.equal(loads, 2);
  }
});
