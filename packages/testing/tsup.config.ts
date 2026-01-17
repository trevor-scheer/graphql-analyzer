import { defineConfig } from "tsup";

export default defineConfig({
  entry: {
    index: "src/index.ts",
    jest: "src/jest.ts",
    vitest: "src/vitest.ts",
    matchers: "src/matchers.ts",
  },
  format: ["cjs", "esm"],
  dts: true,
  clean: true,
  external: ["jest", "vitest", "@graphql-lsp/node"],
});
