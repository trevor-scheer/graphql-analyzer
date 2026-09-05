import assert from "node:assert/strict";
import { test } from "node:test";
import { createRequire } from "node:module";
import { buildSchema, getLocation, parse, print, validate } from "graphql";
import upstream from "@graphql-eslint/eslint-plugin";
const require = createRequire(import.meta.url);
const { convertDocument } = require("../dist/converter.js");

const schemaSdl = `enum Color { RED BLUE } input Filter { color: Color = RED }
  type Query { user(id: ID!, filter: Filter): User } type User { id: ID! name: String }`;
const code = `# eslint-disable example\nquery Named($id: ID!, $filter: Filter = { color: RED }) {
  user(id: $id, filter: $filter) { id name @include(if: true) }
}`;

function walk(node, result = []) {
  if (!node || typeof node !== "object") return result;
  if (Array.isArray(node)) {
    for (const child of node) walk(child, result);
    return result;
  }
  if (typeof node.rawNode === "function") result.push(node);
  for (const [key, child] of Object.entries(node)) {
    if (!["loc", "range", "parent", "leadingComments"].includes(key) && typeof child === "object")
      walk(child, result);
  }
  return result;
}

function normalizedInfo(info) {
  return Object.fromEntries(
    Object.entries(info).map(([key, value]) => [
      key,
      value && typeof value === "object" ? (value.name ?? String(value)) : value,
    ]),
  );
}

test("converted visitors, raw nodes, and all nine TypeInfo fields match upstream", () => {
  const raw = parse(code);
  const schema = buildSchema(schemaSdl);
  const ours = convertDocument(raw, () => schema);
  const theirs = upstream.parser.parseForESLint(code, {
    filePath: "/tmp/query.graphql",
    schemaSdl,
  });
  const actual = walk(ours.root);
  const expected = walk(theirs.ast.body);
  assert.equal(actual.length, expected.length);
  actual.forEach((node, index) => {
    const reference = expected[index];
    assert.equal(node.type, reference.type);
    assert.deepEqual(node.range, reference.range);
    assert.deepEqual(node.leadingComments, reference.leadingComments);
    assert.equal(print(node.rawNode()), print(reference.rawNode()));
    assert.deepEqual(
      normalizedInfo(node.typeInfo()),
      normalizedInfo(reference.typeInfo()),
      node.type,
    );
    assert.equal(Object.keys(node.typeInfo()).length, 9);
    assert.equal(node.typeInfo(), node.typeInfo());
    assert.equal(node.rawNode(), node.rawNode());
  });
  assert.equal(ours.root.rawNode(), raw);
  assert.deepEqual(validate(schema, ours.root.rawNode()), []);
  assert.deepEqual(ours.comments, theirs.ast.comments);
  assert.deepEqual(ours.tokens, theirs.ast.tokens);
  assert(ours.visitorKeys.VariableDefinition.includes("gqlType"));
  assert(!ours.visitorKeys.VariableDefinition.includes("type"));
});

test("SDL nodes retain renamed type children and description comments", () => {
  const source = `"""A user\nwith a name""" type User { "Name" name: String! }`;
  const ours = walk(convertDocument(parse(source), () => null).root);
  const theirs = walk(
    upstream.parser.parseForESLint(source, { filePath: "/tmp/schema.graphql", schemaSdl }).ast.body,
  );
  assert.deepEqual(
    ours.map((node) => [node.type, node.leadingComments]),
    theirs.map((node) => [node.type, node.leadingComments]),
  );
  const field = ours.find((node) => node.type === "FieldDefinition");
  assert.equal(field.gqlType.type, "NonNullType");
  assert.equal(field.gqlType.gqlType.type, "NamedType");
  assert.equal(field.typeInfo(), field.typeInfo());
});

test("locations use UTF-16 offsets and complete multiline token spans", () => {
  const source = `# 😀\r\n"""first\r\nsecond"""\r\ntype Query { name: String }`;
  const document = parse(source);
  const converted = convertDocument(document, () => null);
  for (const node of [...walk(converted.root), ...converted.tokens, ...converted.comments]) {
    const expected = node.range.map((offset) => getLocation(document.loc.source, offset));
    assert.deepEqual(
      node.loc,
      {
        start: { line: expected[0].line, column: expected[0].column - 1 },
        end: { line: expected[1].line, column: expected[1].column - 1 },
      },
    );
  }
});

test("schema and TypeInfo remain lazy for syntax-only visitors", () => {
  let loads = 0;
  const converted = convertDocument(parse(code), () => {
    loads++;
    return buildSchema(schemaSdl);
  });
  const nodes = walk(converted.root);
  assert.equal(loads, 0);
  for (const node of nodes) node.typeInfo();
  assert.equal(loads, 1);
});
