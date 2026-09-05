import type { Linter } from "eslint";
import * as binding from "./binding";

// Extension → language tag accepted by `binding.extractGraphql`. Anything not
// in this map (e.g. `.graphql` itself) goes through unchanged — the file is
// already pure GraphQL and the rules' `Program()` visitor will lint it
// directly via `binding.lintFile`.
const LANG_BY_EXT: Record<string, string> = {
  ".ts": "ts",
  ".tsx": "tsx",
  ".js": "js",
  ".jsx": "jsx",
  ".mjs": "mjs",
  ".cjs": "cjs",
  ".vue": "vue",
  ".svelte": "svelte",
  ".astro": "astro",
};

// Per-file extraction metadata indexed by host filePath. Populated in
// `preprocess`, consumed in `postprocess` to remap diagnostic positions
// from per-block coordinates back to host-file coordinates. Cleared on
// the next `preprocess` call for the same file (so stale entries from a
// previous lint pass don't bleed into a re-lint).
const blocksMeta = new Map<string, Array<{ lineOffset: number; byteOffset: number }>>();

function lineOffsetForByteOffset(source: string, byteOffset: number): number {
  let lineOffset = 0;
  const limit = Math.min(byteOffset, source.length);
  for (let i = 0; i < limit; i++) {
    if (source.charCodeAt(i) === 10 /* \n */) lineOffset++;
  }
  return lineOffset;
}

function extOf(filePath: string): string {
  const dot = filePath.lastIndexOf(".");
  return dot === -1 ? "" : filePath.slice(dot);
}

export const fastProcessor = {
  preprocess(code: string): Array<string> {
    return [code];
  },

  // Map per-block diagnostic coordinates back to host-file coordinates.
  // The last messages array corresponds to the host source itself (which
  // we appended in `preprocess`); it has no extraction metadata so its
  // diagnostics pass through unchanged.
  postprocess(messages: Linter.LintMessage[][], filePath: string): Linter.LintMessage[] {
    const meta = blocksMeta.get(filePath) ?? [];
    const out: Linter.LintMessage[] = [];
    for (let i = 0; i < messages.length; i++) {
      const blockMeta = meta[i];
      for (const msg of messages[i] || []) {
        if (blockMeta) {
          msg.line += blockMeta.lineOffset;
          if (typeof msg.endLine === "number") {
            msg.endLine += blockMeta.lineOffset;
          }
          if (msg.fix) {
            msg.fix.range = [
              msg.fix.range[0] + blockMeta.byteOffset,
              msg.fix.range[1] + blockMeta.byteOffset,
            ];
          }
          // `msg.suggestions` not remapped yet — see PARITY_TODO item 4b.
        }
        out.push(msg);
      }
    }
    return out.sort((a, b) => a.line - b.line || a.column - b.column);
  },

  supportsAutofix: true,
};

export const processor = {
  preprocess(code: string, filename: string): Array<string | { text: string; filename: string }> {
    return (
      require("./compat-processor") as typeof import("./compat-processor")
    ).compatibilityProcessor.preprocess(code, filename);
  },
  postprocess(messages: Linter.LintMessage[][], filename: string): Linter.LintMessage[] {
    return (
      require("./compat-processor") as typeof import("./compat-processor")
    ).compatibilityProcessor.postprocess(messages, filename);
  },
  supportsAutofix: true,
};
