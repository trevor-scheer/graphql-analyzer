import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";

// When wasm-pack hasn't run yet (no .js alongside the .d.ts stub), provide a
// placeholder module so `tsc --noEmit` and `vite build` succeed without the
// real wasm artifact. `xtask web` generates the real file before building.
const WASM_MODULE_ID = resolve(__dirname, "src/wasm/graphql_lsp_wasm.js");
const WASM_STUB_ID = "\0virtual:graphql-lsp-wasm-stub";

function wasmStubPlugin(): Plugin {
  return {
    name: "wasm-stub",
    resolveId(id, _importer, _options) {
      if (id.includes("graphql_lsp_wasm") && !existsSync(WASM_MODULE_ID)) {
        return WASM_STUB_ID;
      }
    },
    load(id) {
      if (id === WASM_STUB_ID) {
        return [
          "export default function init() {",
          "  return Promise.reject(new Error('wasm not built — run xtask web'));",
          "}",
          "export class Server {",
          "  constructor() { throw new Error('wasm not built — run xtask web'); }",
          "  handleMessage(_json) { throw new Error('wasm not built — run xtask web'); }",
          "}",
        ].join("\n");
      }
    },
  };
}

export default defineConfig({
  build: { target: "esnext" },
  worker: { format: "es", plugins: () => [wasmStubPlugin()] },
  optimizeDeps: { exclude: ["monaco-editor"] },
  server: { fs: { allow: [resolve(__dirname, "../..")] } },
  plugins: [wasmStubPlugin()],
});
