import { defineConfig } from "tsup";

export default defineConfig({
  entry: {
    index: "src/index.ts",
    vite: "src/vite.ts",
    webpack: "src/webpack.ts",
    esbuild: "src/esbuild.ts",
  },
  format: ["cjs", "esm"],
  dts: true,
  clean: true,
  external: ["vite", "webpack", "esbuild", "@graphql-lsp/node"],
});
