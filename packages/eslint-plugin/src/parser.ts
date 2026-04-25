import type { Linter } from "eslint";

// ESLint's flat config requires a parser. Our rule shims don't read any AST
// — they delegate to the Rust analyzer — so the parser just produces the
// minimal empty `Program` that satisfies ESLint's machinery.
function lastLineCol(code: string): { line: number; column: number } {
  if (code.length === 0) {
    return { line: 1, column: 0 };
  }
  const lines = code.split("\n");
  return {
    line: lines.length,
    column: lines[lines.length - 1].length,
  };
}

export function parseForESLint(code: string, _options?: Linter.ParserOptions) {
  return {
    ast: {
      type: "Program" as const,
      sourceType: "script" as const,
      body: [] as never[],
      tokens: [] as never[],
      comments: [] as never[],
      loc: { start: { line: 1, column: 0 }, end: lastLineCol(code) },
      range: [0, code.length] as [number, number],
    },
  };
}
