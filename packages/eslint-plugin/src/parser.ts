function lastLineCol(code: string): { line: number; column: number } {
  const lines = code.split("\n");
  return {
    line: lines.length,
    column: lines[lines.length - 1].length,
  };
}

export function parseForESLint(code: string, _options?: Record<string, unknown>) {
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
