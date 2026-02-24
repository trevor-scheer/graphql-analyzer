/**
 * TextMate grammar regression tests for GraphQL syntax highlighting.
 *
 * Uses vscode-textmate + vscode-oniguruma to programmatically tokenize
 * GraphQL source and assert that tokens receive the correct scopes.
 */

import { describe, it, expect, beforeAll } from "vitest";
import oniguruma from "vscode-oniguruma";
import textmate, { type IGrammar, type IToken, type StateStack } from "vscode-textmate";
import { readFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const { createOnigScanner, createOnigString, loadWASM } = oniguruma;
const { Registry, parseRawGrammar } = textmate;

const __dirname = dirname(fileURLToPath(import.meta.url));
const grammarPath = resolve(__dirname, "../syntaxes/graphql.tmLanguage.json");

// Find onig.wasm - npm workspaces hoists to the repo root
const possibleWasmPaths = [
  resolve(__dirname, "../node_modules/vscode-oniguruma/release/onig.wasm"),
  resolve(__dirname, "../../../node_modules/vscode-oniguruma/release/onig.wasm"),
];
const wasmPath = possibleWasmPaths.find((p) => {
  try {
    readFileSync(p);
    return true;
  } catch {
    return false;
  }
});

interface TokenizedLine {
  line: string;
  tokens: IToken[];
}

let grammar: IGrammar;

beforeAll(async () => {
  const wasmBin = readFileSync(wasmPath!).buffer;
  await loadWASM(wasmBin);

  const registry = new Registry({
    onigLib: Promise.resolve({ createOnigScanner, createOnigString }),
    loadGrammar: async (scopeName: string) => {
      if (scopeName === "source.graphql") {
        const content = readFileSync(grammarPath, "utf-8");
        return parseRawGrammar(content, grammarPath);
      }
      return null;
    },
  });

  grammar = (await registry.loadGrammar("source.graphql"))!;
});

function tokenize(lines: string[]): TokenizedLine[] {
  let ruleStack: StateStack | null = null;
  const results: TokenizedLine[] = [];
  for (const line of lines) {
    const result = grammar.tokenizeLine(line, ruleStack);
    ruleStack = result.ruleStack;
    results.push({ line, tokens: result.tokens });
  }
  return results;
}

function expectToken(result: TokenizedLine, text: string, expectedScope: string): void {
  const { line, tokens } = result;
  const token = tokens.find((t) => line.substring(t.startIndex, t.endIndex) === text);
  expect(token, `token "${text}" not found on line: ${line}`).toBeDefined();
  const scopes = token!.scopes.join(" > ");
  expect(scopes, `"${text}" expected scope "${expectedScope}"`).toContain(expectedScope);
}

// Regression: body-less type extensions broke highlighting on subsequent lines
// because the implements sub-pattern crossed line boundaries, causing a double
// zero-width end that prevented the engine from re-entering graphql-type-interface.
describe("body-less type extensions", () => {
  it("extend type followed by extend interface", () => {
    const t = tokenize([
      "extend type User implements Node",
      "",
      "extend interface Node {",
      "  createdAt: String!",
      "}",
    ]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "type", "keyword.type.graphql");
    expectToken(t[0], "User", "support.type.graphql");
    expectToken(t[0], "implements", "keyword.implements.graphql");
    expectToken(t[0], "Node", "support.type.graphql");
    expectToken(t[2], "extend", "keyword.type.graphql");
    expectToken(t[2], "interface", "keyword.interface.graphql");
    expectToken(t[2], "Node", "support.type.graphql");
    expectToken(t[2], "{", "punctuation.operation.graphql");
    expectToken(t[3], "createdAt", "variable.graphql");
    expectToken(t[3], "String", "support.type.builtin.graphql");
  });

  it("multiple consecutive body-less extensions", () => {
    const t = tokenize([
      "extend type Foo implements Bar",
      "extend type Baz implements Qux",
      "extend interface Node {",
      "  id: ID!",
      "}",
    ]);
    expectToken(t[0], "Foo", "support.type.graphql");
    expectToken(t[0], "implements", "keyword.implements.graphql");
    expectToken(t[0], "Bar", "support.type.graphql");
    expectToken(t[1], "extend", "keyword.type.graphql");
    expectToken(t[1], "Baz", "support.type.graphql");
    expectToken(t[1], "Qux", "support.type.graphql");
    expectToken(t[2], "interface", "keyword.interface.graphql");
    expectToken(t[2], "{", "punctuation.operation.graphql");
  });

  // Regression: the graphql-directive pattern included graphql-skip-newlines
  // which consumed the newline, causing the directive to cross line boundaries
  // and produce the same double zero-width end issue.
  it("with directive (no implements)", () => {
    const t = tokenize(["extend type User @deprecated", "type Post {", "  title: String", "}"]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "User", "support.type.graphql");
    expectToken(t[0], "@deprecated", "entity.name.function.directive.graphql");
    expectToken(t[1], "type", "keyword.type.graphql");
    expectToken(t[1], "Post", "support.type.graphql");
    expectToken(t[1], "{", "punctuation.operation.graphql");
    expectToken(t[2], "title", "variable.graphql");
  });

  it("bare (no implements or directive)", () => {
    const t = tokenize(["extend type User", "type Post {", "  title: String", "}"]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "User", "support.type.graphql");
    expectToken(t[1], "type", "keyword.type.graphql");
    expectToken(t[1], "Post", "support.type.graphql");
    expectToken(t[1], "{", "punctuation.operation.graphql");
  });

  it("followed by enum", () => {
    const t = tokenize([
      "extend type User implements Node",
      "enum Color {",
      "  RED",
      "  BLUE",
      "}",
    ]);
    expectToken(t[1], "enum", "keyword.enum.graphql");
    expectToken(t[1], "Color", "support.type.enum.graphql");
  });
});

// Regression: the begin pattern only applied (extends?) to the type
// alternative, not interface or input.
describe("extend keyword on all definition types", () => {
  it("extend interface and extend input", () => {
    const t = tokenize([
      "extend interface Node {",
      "  id: ID!",
      "}",
      "extend input CreateUserInput {",
      "  email: String!",
      "}",
    ]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "interface", "keyword.interface.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
    expectToken(t[3], "extend", "keyword.type.graphql");
    expectToken(t[3], "input", "keyword.input.graphql");
    expectToken(t[3], "{", "punctuation.operation.graphql");
  });

  it("extend enum", () => {
    const t = tokenize(["extend enum Role {", "  MODERATOR", "  GUEST", "}"]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "enum", "keyword.enum.graphql");
    expectToken(t[0], "Role", "support.type.enum.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
  });

  it("extend union", () => {
    const t = tokenize(["extend union SearchResult = Post"]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "union", "keyword.union.graphql");
    expectToken(t[0], "SearchResult", "support.type.graphql");
    expectToken(t[0], "Post", "support.type.graphql");
  });

  it("extend enum followed by extend input", () => {
    const t = tokenize([
      "extend enum Role {",
      "  MODERATOR",
      "}",
      "extend input CreateUserInput {",
      "  role: Role",
      "}",
    ]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "enum", "keyword.enum.graphql");
    expectToken(t[3], "extend", "keyword.type.graphql");
    expectToken(t[3], "input", "keyword.input.graphql");
    expectToken(t[3], "CreateUserInput", "support.type.graphql");
  });
});

describe("type definitions with body", () => {
  it("type with implements and body", () => {
    const t = tokenize(["type User implements Node {", "  name: String!", "}"]);
    expectToken(t[0], "type", "keyword.type.graphql");
    expectToken(t[0], "User", "support.type.graphql");
    expectToken(t[0], "implements", "keyword.implements.graphql");
    expectToken(t[0], "Node", "support.type.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
    expectToken(t[1], "name", "variable.graphql");
    expectToken(t[1], "String", "support.type.builtin.graphql");
  });
});

describe("directives", () => {
  it("directive with arguments on type", () => {
    const t = tokenize(["type User @cacheControl(maxAge: 300) {", "  name: String", "}"]);
    expectToken(t[0], "@cacheControl", "entity.name.function.directive.graphql");
    expectToken(t[0], "maxAge", "variable.parameter.graphql");
    expectToken(t[0], "300", "constant.numeric.float.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
    expectToken(t[1], "name", "variable.graphql");
  });

  it("field directive followed by next field", () => {
    const t = tokenize(["type User {", "  name: String @deprecated", "  age: Int", "}"]);
    expectToken(t[1], "@deprecated", "entity.name.function.directive.graphql");
    expectToken(t[2], "age", "variable.graphql");
    expectToken(t[2], "Int", "support.type.builtin.graphql");
  });

  it("multiple directives on type", () => {
    const t = tokenize([
      "type User @deprecated @cacheControl(maxAge: 300) {",
      "  name: String",
      "}",
    ]);
    expectToken(t[0], "@deprecated", "entity.name.function.directive.graphql");
    expectToken(t[0], "@cacheControl", "entity.name.function.directive.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
    expectToken(t[1], "name", "variable.graphql");
  });

  it("query with directive", () => {
    const t = tokenize(["query GetUser @client {", "  user {", "    name", "  }", "}"]);
    expectToken(t[0], "query", "keyword.operation.graphql");
    expectToken(t[0], "GetUser", "entity.name.function.graphql");
    expectToken(t[0], "@client", "entity.name.function.directive.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
    expectToken(t[2], "name", "variable.graphql");
  });
});
