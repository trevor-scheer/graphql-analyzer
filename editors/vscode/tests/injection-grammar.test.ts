/**
 * TextMate injection grammar tests for GraphQL in TypeScript/JavaScript.
 *
 * Tests the `inline.graphql` grammar that injects GraphQL highlighting into
 * TS/JS files for `graphql(...)`, `gql(...)`, and tagged template patterns.
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
const syntaxesDir = resolve(__dirname, "../syntaxes");
const graphqlGrammarPath = resolve(syntaxesDir, "graphql.tmLanguage.json");
const injectionGrammarPath = resolve(syntaxesDir, "graphql.injection.tmLanguage.json");

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

// Minimal TypeScript grammar — just enough to serve as a host for injections.
// The injection grammar's patterns will match before these (L: selector).
const minimalTsGrammar = {
  scopeName: "source.ts",
  patterns: [
    { match: "\\b(const|let|var|function|import|from|export)\\b", name: "keyword.ts" },
    { match: ";", name: "punctuation.terminator.statement.ts" },
  ],
};

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
        return parseRawGrammar(readFileSync(graphqlGrammarPath, "utf-8"), graphqlGrammarPath);
      }
      if (scopeName === "source.ts") {
        return parseRawGrammar(JSON.stringify(minimalTsGrammar), "source.ts.json");
      }
      if (scopeName === "inline.graphql") {
        return parseRawGrammar(readFileSync(injectionGrammarPath, "utf-8"), injectionGrammarPath);
      }
      return null;
    },
    getInjections: (scopeName: string) => {
      if (scopeName === "source.ts") {
        return ["inline.graphql"];
      }
      return undefined;
    },
  });

  grammar = (await registry.loadGrammar("source.ts"))!;
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

function findToken(result: TokenizedLine, text: string): IToken | undefined {
  const { line, tokens } = result;
  return tokens.find((t) => line.substring(t.startIndex, t.endIndex) === text);
}

function expectToken(result: TokenizedLine, text: string, expectedScope: string): void {
  const token = findToken(result, text);
  expect(token, `token "${text}" not found on line: ${result.line}`).toBeDefined();
  const scopes = token!.scopes.join(" > ");
  expect(scopes, `"${text}" expected scope "${expectedScope}"`).toContain(expectedScope);
}

function expectScopeAtText(result: TokenizedLine, text: string, expectedScope: string): void {
  const { line, tokens } = result;
  const idx = line.indexOf(text);
  expect(idx, `"${text}" not found in line: ${line}`).not.toBe(-1);
  const token = tokens.find((t) => t.startIndex <= idx && t.endIndex >= idx + text.length);
  expect(token, `no token spanning "${text}" on line: ${line}`).toBeDefined();
  const scopes = token!.scopes.join(" > ");
  expect(scopes, `"${text}" expected scope "${expectedScope}"`).toContain(expectedScope);
}

describe("function call with template literal (same line)", () => {
  it("graphql(`...`) parentheses get meta.brace.round.ts scope", () => {
    const t = tokenize(["graphql(`query { user { name } }`)"]);
    expectToken(t[0], "graphql", "entity.name.function.js");
    expectToken(t[0], "(", "meta.brace.round.ts");
    expectToken(t[0], ")", "meta.brace.round.ts");
  });

  it("backticks get template string scopes", () => {
    const t = tokenize(["graphql(`query { user { name } }`)"]);
    const line = t[0];
    const openBacktick = line.tokens.find(
      (tk) => line.line[tk.startIndex] === "`" && tk.scopes.join(" ").includes("template.begin"),
    );
    const closeBacktick = line.tokens.find(
      (tk) => line.line[tk.startIndex] === "`" && tk.scopes.join(" ").includes("template.end"),
    );
    expect(openBacktick, "opening backtick not found").toBeDefined();
    expect(closeBacktick, "closing backtick not found").toBeDefined();
  });

  it("GraphQL content inside gets embedded graphql scope", () => {
    const t = tokenize(["graphql(`query { user { name } }`)"]);
    expectScopeAtText(t[0], "query", "meta.embedded.block.graphql");
  });

  it("gql(`...`) works the same as graphql(`...`)", () => {
    const t = tokenize(["gql(`{ user { name } }`)"]);
    expectToken(t[0], "gql", "entity.name.function.js");
    expectToken(t[0], "(", "meta.brace.round.ts");
    expectToken(t[0], ")", "meta.brace.round.ts");
  });
});

describe("function call with template literal (multi-line)", () => {
  it("graphql(\\n`...`) parentheses get meta.brace.round.ts scope", () => {
    // When `)` is on the same line as the closing backtick, it gets scoped
    const t = tokenize(["graphql(", "  `query { user { name } }`)"]);
    expectToken(t[0], "graphql", "entity.name.function.js");
    expectToken(t[0], "(", "meta.brace.round.ts");
    expectToken(t[1], ")", "meta.brace.round.ts");
  });

  it("backticks get template string scopes on inner line", () => {
    const t = tokenize(["graphql(", "  `query { user { name } }`", ")"]);
    const innerLine = t[1];
    const openBacktick = innerLine.tokens.find(
      (tk) =>
        innerLine.line[tk.startIndex] === "`" && tk.scopes.join(" ").includes("template.begin"),
    );
    expect(openBacktick, "opening backtick not found on inner line").toBeDefined();
  });

  it("GraphQL content inside multi-line gets embedded scope", () => {
    const t = tokenize([
      "graphql(",
      "  `query {",
      "    user {",
      "      name",
      "    }",
      "  }`",
      ")",
    ]);
    expectScopeAtText(t[2], "user", "meta.embedded.block.graphql");
  });
});

describe("tagged template literal (no parentheses)", () => {
  it("gql`...` gets tagged-template scope", () => {
    const t = tokenize(["gql`query { user { name } }`"]);
    expectToken(t[0], "gql", "entity.name.function.tagged-template.js");
  });

  it("graphql`...` gets tagged-template scope", () => {
    const t = tokenize(["graphql`query { user { name } }`"]);
    expectToken(t[0], "graphql", "entity.name.function.tagged-template.js");
  });

  it("tagged template has no brace scopes (no parens to scope)", () => {
    const t = tokenize(["gql`query { user { name } }`"]);
    const hasBrace = t[0].tokens.some((tk) =>
      tk.scopes.some((s) => s.includes("meta.brace.round")),
    );
    expect(hasBrace, "tagged template should not have meta.brace.round").toBe(false);
  });
});

describe("tagged template literal (multi-line)", () => {
  it("gql\\n`...` matches across lines", () => {
    const t = tokenize(["gql", "`query { user { name } }`"]);
    expectToken(t[0], "gql", "entity.name.function.tagged-template.js");
    expectScopeAtText(t[1], "query", "meta.embedded.block.graphql");
  });
});

describe("template substitution in injection", () => {
  it("${...} inside graphql(`...`) gets interpolation scope", () => {
    const t = tokenize(["graphql(`query { ${fragment} }`)"]);
    // The GraphQL grammar handles ${...} as native.interpolation
    expectScopeAtText(t[0], "${", "keyword.other.substitution.begin");
  });
});
