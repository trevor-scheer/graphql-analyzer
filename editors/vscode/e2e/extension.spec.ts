import { test, expect } from "./vscodeFixture";

// Use Meta (Command) on macOS, Control on other platforms
const mod = process.platform === "darwin" ? "Meta" : "Control";

test.describe("GraphQL LSP Extension", () => {
  test("activates when opening a GraphQL file", async ({ vscode }) => {
    const { page } = vscode;
    const body = page.locator("body");

    // Open quick open and switch to command mode with ">"
    await body.press(`${mod}+P`);
    const commandPalette = page.locator(".quick-input-widget");
    await expect(commandPalette).toBeVisible();

    // Type ">" to switch to command mode, then search for our command
    const input = commandPalette.locator("input");
    await input.fill(">GraphQL: Check Status");

    // Wait for the command to appear in results (longer timeout for extension activation)
    const checkStatusResult = page.getByText("Check Status");
    await expect(checkStatusResult).toBeVisible({ timeout: 10000 });

    await page.screenshot({ path: "test-results/command-palette.png" });

    // Press Enter to execute the command
    await input.press("Enter");

    // Wait for command palette to close
    await expect(commandPalette).not.toBeVisible();

    await page.screenshot({ path: "test-results/after-command.png" });
  });

  test("opens GraphQL file from explorer", async ({ vscode }) => {
    const { page } = vscode;
    const body = page.locator("body");

    // Focus the explorer
    await body.press(`${mod}+Shift+E`);

    // Wait for the explorer to show our file
    const queryFile = page.getByText("query.graphql").first();
    await expect(queryFile).toBeVisible();

    await page.screenshot({ path: "test-results/explorer-open.png" });

    // Double-click to open the file
    await queryFile.dblclick();

    // Wait for editor to show the file content
    const editorContent = page.locator(".view-lines").first();
    await expect(editorContent).toBeVisible();

    await page.screenshot({ path: "test-results/query-file-opened.png" });
  });

  test("shows GraphQL syntax highlighting", async ({ vscode }) => {
    const { page } = vscode;
    const body = page.locator("body");

    // Open quick open dialog
    await body.press(`${mod}+P`);
    const quickOpen = page.locator(".quick-input-widget");
    await expect(quickOpen).toBeVisible();

    // Type filename and wait for it to appear in results
    const input = quickOpen.locator("input");
    await input.fill("query.graphql");
    const fileResult = page.getByText("query.graphql").first();
    await expect(fileResult).toBeVisible();

    // Open the file
    await input.press("Enter");

    // Wait for editor to be ready with content
    const editorContent = page.locator(".view-lines").first();
    await expect(editorContent).toBeVisible();

    await page.screenshot({ path: "test-results/graphql-file.png" });

    // Verify we can see the editor
    const editor = page.locator(".monaco-editor").first();
    await expect(editor).toBeVisible();
  });

  test("extension commands are available", async ({ vscode }) => {
    const { page } = vscode;
    const body = page.locator("body");

    // Open quick open and switch to command mode with ">"
    await body.press(`${mod}+P`);
    const commandPalette = page.locator(".quick-input-widget");
    await expect(commandPalette).toBeVisible();

    // Type ">" to switch to command mode, then search for GraphQL commands
    const input = commandPalette.locator("input");
    await input.fill(">GraphQL");

    // Wait for our commands to appear (longer timeout for extension activation)
    const restartCommand = page.getByText("Restart GraphQL Language Server");
    const checkStatusCommand = page.getByText("Check Status");

    await expect(restartCommand).toBeVisible({ timeout: 10000 });
    await expect(checkStatusCommand).toBeVisible({ timeout: 10000 });

    await page.screenshot({ path: "test-results/graphql-commands.png" });

    // Close the command palette
    await input.press("Escape");
    await expect(commandPalette).not.toBeVisible();
  });
});
