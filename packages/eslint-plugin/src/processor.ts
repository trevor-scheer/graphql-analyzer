import type { Linter } from "eslint";

export const fastProcessor = {
  preprocess(code: string): Array<string> {
    return [code];
  },

  postprocess(messages: Linter.LintMessage[][]): Linter.LintMessage[] {
    return messages.flat();
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
