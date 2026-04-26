import { defineConfig } from "vite";

export default defineConfig({
  worker: { format: "es" },
  optimizeDeps: { exclude: ["monaco-editor"] },
  server: { fs: { allow: [".."] } },
});
