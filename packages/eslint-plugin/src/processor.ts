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

export const processor = {
  // ESLint v9 flat config dispatches processors via `meta.name`; without it
  // the processor is treated as legacy and may not fire correctly.
  meta: {
    name: "@graphql-analyzer/processor",
  },
  // ESLint runs `preprocess` before parsing. We extract embedded GraphQL
  // blocks from JS/TS-family hosts and present each block to ESLint as a
  // virtual `.graphql` file. The host's original source comes last so other
  // ESLint configs/parsers (TypeScript, React, etc.) still run on it.
  // Pure-GraphQL files have no extraction work to do — return identity so
  // the rule's `Program()` visitor handles them directly.
  preprocess(code: string, filePath: string): Array<string | { filename: string; text: string }> {
    const lang = LANG_BY_EXT[extOf(filePath)];
    if (!lang) {
      blocksMeta.delete(filePath);
      return [code];
    }
    let extracted: binding.JsExtractedBlock[];
    try {
      extracted = binding.extractGraphql(code, lang);
    } catch {
      // Extraction failure (e.g. malformed JS) shouldn't block ESLint from
      // running its other rules on the host file — drop GraphQL coverage
      // for this file silently and let the rest of the lint pass continue.
      blocksMeta.delete(filePath);
      return [code];
    }
    if (extracted.length === 0) {
      blocksMeta.delete(filePath);
      return [code];
    }
    const meta: Array<{ lineOffset: number; byteOffset: number }> = [];
    const blocks: Array<{ filename: string; text: string }> = [];
    for (let i = 0; i < extracted.length; i++) {
      const block = extracted[i];
      meta.push({
        lineOffset: lineOffsetForByteOffset(code, block.offset),
        byteOffset: block.offset,
      });
      // Numeric prefix keeps virtual filenames unique per block. The
      // `.graphql` suffix is what ESLint matches against the user's
      // `files: ["**/*.graphql"]` config block — that's where our parser
      // and rules are wired.
      blocks.push({ filename: `${i}_document.graphql`, text: block.source });
    }
    blocksMeta.set(filePath, meta);
    return [...blocks, code];
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
