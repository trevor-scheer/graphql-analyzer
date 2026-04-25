import type { Linter } from "eslint";

// Identity processor. Our rule shims call into the Rust analyzer, which
// handles embedded-GraphQL extraction from JS/TS/Vue/Svelte/Astro internally
// and emits diagnostics at the original source position. We don't virtualize
// each embedded block as a separate `.graphql` file (graphql-eslint's
// approach) because that requires a postprocess-time position remap — a
// follow-up once the plugin's feature parity is locked down.
export const processor = {
  preprocess(code: string): Array<string> {
    return [code];
  },

  postprocess(messages: Linter.LintMessage[][]): Linter.LintMessage[] {
    return messages.flat();
  },

  supportsAutofix: true,
};
