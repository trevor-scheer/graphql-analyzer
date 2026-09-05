import type { Linter } from "eslint";
import { name, version } from "./meta";

export const fastProcessor = {
  meta: { name: `${name}/fast-processor`, version },
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
  meta: { name: `${name}/processor`, version },
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
