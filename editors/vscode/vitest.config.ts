import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["**/*.test.ts"],
    exclude: ["node_modules", "e2e", "out", "out-e2e", ".vscode-test"],
  },
});
