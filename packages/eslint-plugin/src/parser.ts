import type { ParserOptions, GraphQLESLintParseResult } from "./types";
import { name, version } from "./meta";

const compatibilityPrograms = new WeakSet<object>();

export function isCompatibilityProgram(program: object): boolean {
  return compatibilityPrograms.has(program);
}

export function parseForESLint(
  code: string,
  options: ParserOptions = {},
): GraphQLESLintParseResult {
  const result = (require("./compat-parser") as typeof import("./compat-parser")).parseForESLint(
    code,
    options,
  );
  compatibilityPrograms.add(result.ast);
  return result;
}

export const fastParser = {
  meta: { name: `${name}/fast-parser`, version },
  parseForESLint(code: string) {
    const lines = code.split(/\r\n|[\n\r]/u);
    return {
      ast: {
        type: "Program" as const,
        sourceType: "script" as const,
        body: [] as never[],
        tokens: [] as never[],
        comments: [] as never[],
        loc: {
          start: { line: 1, column: 0 },
          end: { line: lines.length, column: lines.at(-1)!.length },
        },
        range: [0, code.length] as [number, number],
      },
    };
  },
};
