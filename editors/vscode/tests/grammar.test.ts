/**
 * TextMate grammar regression tests for GraphQL syntax highlighting.
 *
 * Uses vscode-textmate + vscode-oniguruma to programmatically tokenize
 * GraphQL source and assert that tokens receive the correct scopes.
 */

import { describe, it, expect, beforeAll } from "vitest";
import oniguruma from "vscode-oniguruma";
import textmate, { type IGrammar, type IToken, type StateStack } from "vscode-textmate";
import { readFileSync, readdirSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const { createOnigScanner, createOnigString, loadWASM } = oniguruma;
const { Registry, parseRawGrammar } = textmate;

const __dirname = dirname(fileURLToPath(import.meta.url));
const syntaxesDir = resolve(__dirname, "../syntaxes");
const grammarPath = resolve(syntaxesDir, "graphql.tmLanguage.json");

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

/** Find the scope at the position where `text` appears in the line (handles tokens with leading whitespace). */
function expectScopeAtText(result: TokenizedLine, text: string, expectedScope: string): void {
  const { line, tokens } = result;
  const idx = line.indexOf(text);
  expect(idx, `"${text}" not found in line: ${line}`).not.toBe(-1);
  const token = tokens.find((t) => t.startIndex <= idx && t.endIndex >= idx + text.length);
  expect(token, `no token spanning "${text}" on line: ${line}`).toBeDefined();
  const scopes = token!.scopes.join(" > ");
  expect(scopes, `"${text}" expected scope "${expectedScope}"`).toContain(expectedScope);
}

/** Assert that no token on the line has the given scope for text at its position. */
function expectNoScopeAtText(result: TokenizedLine, text: string, forbiddenScope: string): void {
  const { line, tokens } = result;
  const idx = line.indexOf(text);
  expect(idx, `"${text}" not found in line: ${line}`).not.toBe(-1);
  const token = tokens.find((t) => t.startIndex <= idx && t.endIndex >= idx + text.length);
  if (token) {
    const scopes = token.scopes.join(" > ");
    expect(scopes, `"${text}" should NOT have scope "${forbiddenScope}"`).not.toContain(
      forbiddenScope,
    );
  }
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
    expectToken(t[0], "300", "constant.numeric.graphql");
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

// #743: subscription keyword was missing from graphql-query-mutation pattern
describe("subscription keyword (#743)", () => {
  it("subscription gets keyword.operation scope", () => {
    const t = tokenize(["subscription OnUserCreated {", "  userCreated {", "    name", "  }", "}"]);
    expectToken(t[0], "subscription", "keyword.operation.graphql");
    expectToken(t[0], "OnUserCreated", "entity.name.function.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
    expectToken(t[2], "name", "variable.graphql");
  });

  it("anonymous subscription", () => {
    const t = tokenize(["subscription {", "  newMessage", "}"]);
    expectToken(t[0], "subscription", "keyword.operation.graphql");
    expectToken(t[0], "{", "punctuation.operation.graphql");
  });
});

// #743: graphql-comment had """ and " sub-patterns that shadowed string/description syntax.
// Before the fix, strings like (name: "hello") were tokenized as comments.
describe("strings vs comments (#743)", () => {
  it("string argument values are scoped as strings, not comments", () => {
    const t = tokenize(["query {", '  user(name: "hello") {', "    id", "  }", "}"]);
    // The grammar splits strings into begin-quote, content, end-quote tokens.
    // Check that the content portion gets string scope, not comment scope.
    expectScopeAtText(t[1], "hello", "string.quoted.double.graphql");
    expectNoScopeAtText(t[1], "hello", "comment");
  });

  it("triple-quoted descriptions get description scope, not comment.line.graphql", () => {
    const t = tokenize(['"""A user type"""', "type User {", "  name: String", "}"]);
    // Description docstrings use comment.block.graphql, not comment.line.graphql
    expectScopeAtText(t[0], "A user type", "comment");
    expectNoScopeAtText(t[0], "A user type", "comment.line.graphql");
    expectToken(t[1], "type", "keyword.type.graphql");
    expectToken(t[1], "User", "support.type.graphql");
  });

  it("single-line string descriptions do not shadow subsequent definitions", () => {
    const t = tokenize(['"A user type"', "type User {", "  name: String", "}"]);
    // The key fix: the old graphql-comment pattern had a "..." sub-pattern that
    // would consume the string as a comment, preventing subsequent lines from parsing.
    // After the fix, the string may or may not match a description pattern, but
    // the important thing is that the next line's type definition still parses.
    expectToken(t[1], "type", "keyword.type.graphql");
    expectToken(t[1], "User", "support.type.graphql");
  });
});

// #743: comment scope had .js suffix (comment.line.graphql.js → comment.line.graphql)
describe("comment scope name (#743)", () => {
  it("line comment has comment.line.graphql scope (no .js suffix)", () => {
    const t = tokenize(["# this is a comment"]);
    const token = t[0].tokens.find((tk) =>
      t[0].line.substring(tk.startIndex, tk.endIndex).includes("#"),
    );
    expect(token).toBeDefined();
    const scopes = token!.scopes.join(" ");
    expect(scopes).toContain("comment.line.graphql");
    expect(scopes).not.toContain("comment.line.graphql.js");
  });
});

// #743: enum value lookahead used (?!=...) instead of (?!...).
// The broken regex (?!=...) is "not followed by =" then literal "...", which
// doesn't actually exclude true/false/null. The fix uses proper (?!...).
describe("enum value negative lookahead (#743)", () => {
  it("enum member values get enum scope", () => {
    const t = tokenize(["enum Status {", "  ACTIVE", "  INACTIVE", "}"]);
    // Enum values include leading whitespace in the token due to \\s* in the pattern
    expectScopeAtText(t[1], "ACTIVE", "constant.character.enum.graphql");
    expectScopeAtText(t[2], "INACTIVE", "constant.character.enum.graphql");
  });

  it("boolean literals get boolean scope, not enum scope", () => {
    const t = tokenize(["type Query {", "  enabled(flag: Boolean = true): String", "}"]);
    expectScopeAtText(t[1], "true", "constant.language.boolean.graphql");
  });
});

// #743: directive-definition had phantom beginCaptures 3-5 that didn't match any groups
describe("directive definition (#743)", () => {
  it("directive definition tokenizes correctly without phantom captures", () => {
    const t = tokenize([
      "directive @cacheControl(maxAge: Int) repeatable on FIELD_DEFINITION | OBJECT",
    ]);
    expectToken(t[0], "directive", "keyword.directive.graphql");
    expectToken(t[0], "@cacheControl", "entity.name.function.directive.graphql");
    expectToken(t[0], "maxAge", "variable.parameter.graphql");
    expectToken(t[0], "Int", "support.type.builtin.graphql");
  });

  it("simple directive definition", () => {
    const t = tokenize(["directive @deprecated(reason: String) on FIELD_DEFINITION"]);
    expectToken(t[0], "directive", "keyword.directive.graphql");
    expectToken(t[0], "@deprecated", "entity.name.function.directive.graphql");
    expectToken(t[0], "reason", "variable.parameter.graphql");
  });
});

// #743: scalar now supports extend scalar, consistent with type/enum/union
describe("extend scalar (#743)", () => {
  it("extend scalar gets keyword and entity scopes", () => {
    const t = tokenize(["extend scalar JSON"]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "scalar", "keyword.scalar.graphql");
    expectToken(t[0], "JSON", "entity.scalar.graphql");
  });

  it("plain scalar still works", () => {
    const t = tokenize(["scalar DateTime"]);
    expectToken(t[0], "scalar", "keyword.scalar.graphql");
    expectToken(t[0], "DateTime", "entity.scalar.graphql");
  });

  it("extend scalar followed by another definition", () => {
    const t = tokenize(["extend scalar JSON", "type User {", "  name: String", "}"]);
    expectToken(t[0], "extend", "keyword.type.graphql");
    expectToken(t[0], "JSON", "entity.scalar.graphql");
    expectToken(t[1], "type", "keyword.type.graphql");
    expectToken(t[1], "User", "support.type.graphql");
  });
});

// #743: numeric scope renamed from constant.numeric.float.graphql to constant.numeric.graphql
describe("numeric scope name (#743)", () => {
  it("integer gets constant.numeric.graphql scope", () => {
    const t = tokenize(["query {", "  user(id: 42) {", "    name", "  }", "}"]);
    expectToken(t[1], "42", "constant.numeric.graphql");
  });

  it("float gets constant.numeric.graphql scope", () => {
    const t = tokenize(["query {", "  product(price: 19.99) {", "    name", "  }", "}"]);
    expectToken(t[1], "19.99", "constant.numeric.graphql");
  });
});

// Structural validation: every `#name` include must reference a pattern
// defined in the same grammar's repository. Catches broken/dead references
// like the `#literal-quasi-embedded` issue (#651).
describe("grammar structural validity", () => {
  interface TmGrammar {
    repository?: Record<string, unknown>;
  }

  function collectIncludes(obj: unknown): { ref: string; path: string }[] {
    const results: { ref: string; path: string }[] = [];

    function walk(node: unknown, path: string): void {
      if (node === null || typeof node !== "object") return;

      if (Array.isArray(node)) {
        node.forEach((item, i) => walk(item, `${path}[${i}]`));
        return;
      }

      const record = node as Record<string, unknown>;
      if (typeof record.include === "string" && record.include.startsWith("#")) {
        results.push({ ref: record.include.slice(1), path });
      }
      for (const [key, value] of Object.entries(record)) {
        walk(value, path ? `${path}.${key}` : key);
      }
    }

    walk(obj, "");
    return results;
  }

  const grammarFiles = readdirSync(syntaxesDir)
    .filter((f) => f.endsWith(".tmLanguage.json"))
    .map((f) => [f, resolve(syntaxesDir, f)] as const);

  for (const [filename, filepath] of grammarFiles) {
    it(`${filename}: all #includes resolve to defined patterns`, () => {
      const grammar: TmGrammar = JSON.parse(readFileSync(filepath, "utf-8"));
      const repositoryKeys = new Set(Object.keys(grammar.repository ?? {}));
      const includes = collectIncludes(grammar);
      const broken = includes.filter((inc) => !repositoryKeys.has(inc.ref));

      expect(broken, `broken includes in ${filename}`).toEqual([]);
    });
  }
});
