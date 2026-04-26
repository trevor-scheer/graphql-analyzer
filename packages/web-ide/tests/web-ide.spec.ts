import { test, expect } from "@playwright/test";

// The wasm bundle is large (~22MB in debug builds), so loading can take a while.
// We poll for readiness signals rather than using fixed sleeps.
const WASM_INIT_TIMEOUT = 60_000;

async function waitForLspReady(page: Parameters<typeof test>[1]["page"]) {
  // Poll until window.__monaco is available and __lspReady is set.
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          return (window as unknown as Record<string, unknown>).__lspReady === true;
        }),
      { timeout: WASM_INIT_TIMEOUT, message: "timed out waiting for LSP to initialize" },
    )
    .toBe(true);
}

test("editors mount and worker initializes", async ({ page }) => {
  const messages: string[] = [];
  page.on("console", (m) => messages.push(m.text()));

  await page.goto("/");
  await expect(page.locator(".monaco-editor").first()).toBeVisible();
  await expect(page.locator(".monaco-editor").nth(1)).toBeVisible();

  await waitForLspReady(page);

  // At this point the LSP has responded to the initialize request.
  expect(messages.find((m) => /initialize|capabilities|GraphQL|LSP/i.test(m))).toBeTruthy();
});

test("typing a syntax error produces a diagnostic marker", async ({ page }) => {
  await page.goto("/");

  // Wait for both editors to mount.
  await expect(page.locator(".monaco-editor").first()).toBeVisible();
  await expect(page.locator(".monaco-editor").nth(1)).toBeVisible();

  // Wait for the LSP worker to fully initialize (wasm loading can be slow).
  await waitForLspReady(page);

  // Click the document editor (second Monaco instance) and append a syntax error.
  const docEditor = page.locator(".monaco-editor").nth(1);
  await docEditor.click();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  // Intentionally unclosed brace — guaranteed parser-level syntax error.
  // Schema-aware field validation may not work under wasm yet (Task 19 limitation)
  // but syntax errors are always surfaced.
  await page.keyboard.type('query Bad { user(id: "1") {');

  // Poll for Monaco markers via the exposed window.__monaco global.
  await expect
    .poll(
      async () => {
        const markers = await page.evaluate(() => {
          const m = (window as unknown as Record<string, unknown>).__monaco;
          if (!m) return 0;
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          return (m as any).editor.getModelMarkers({}).length;
        });
        return markers;
      },
      { timeout: 10_000, message: "expected at least one LSP diagnostic marker" },
    )
    .toBeGreaterThan(0);
});
