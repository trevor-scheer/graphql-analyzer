import { defineConfig } from "@playwright/test";

export default defineConfig({
  // The debug wasm bundle is large (~22MB), so allow enough time for loading.
  timeout: 90_000,
  webServer: {
    command: "npm run dev",
    url: "http://localhost:5173",
    reuseExistingServer: true,
    timeout: 60_000,
  },
  use: { baseURL: "http://localhost:5173" },
  testDir: "./tests",
});
