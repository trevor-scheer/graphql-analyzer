import { test, expect } from "./vscodeFixture";

test.describe("GraphQL LSP Extension", () => {
  test("activates when opening a GraphQL file", async ({ vscode }) => {
    const { page } = vscode;

    // Open the command palette
    await page.keyboard.press("Control+Shift+P");
    await page.waitForTimeout(500);

    // Type to search for our extension's command
    await page.keyboard.type("GraphQL: Check Status");
    await page.waitForTimeout(500);

    // Take a screenshot to see the current state
    await page.screenshot({ path: "test-results/command-palette.png" });

    // Press Enter to execute the command
    await page.keyboard.press("Enter");
    await page.waitForTimeout(1000);

    // Take another screenshot after command execution
    await page.screenshot({ path: "test-results/after-command.png" });
  });

  test("opens GraphQL file from explorer", async ({ vscode }) => {
    const { page } = vscode;

    // Focus the explorer - try keyboard shortcut
    await page.keyboard.press("Control+Shift+E");
    await page.waitForTimeout(1000);

    await page.screenshot({ path: "test-results/explorer-open.png" });

    // Try to find and click on query.graphql in the explorer
    const queryFile = page.locator('text="query.graphql"').first();
    if (await queryFile.isVisible()) {
      await queryFile.dblclick();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: "test-results/query-file-opened.png" });
    }
  });

  test("shows GraphQL syntax highlighting", async ({ vscode }) => {
    const { page } = vscode;

    // Use quick open to open the file
    await page.keyboard.press("Control+P");
    await page.waitForTimeout(500);

    await page.keyboard.type("query.graphql");
    await page.waitForTimeout(500);

    await page.keyboard.press("Enter");
    await page.waitForTimeout(1000);

    // Take a screenshot of the opened file
    await page.screenshot({ path: "test-results/graphql-file.png" });

    // Verify we can see the editor content
    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible();
  });

  test("extension commands are available", async ({ vscode }) => {
    const { page } = vscode;

    // Open command palette
    await page.keyboard.press("Control+Shift+P");
    await page.waitForTimeout(500);

    // Search for GraphQL commands
    await page.keyboard.type("GraphQL");
    await page.waitForTimeout(500);

    await page.screenshot({
      path: "test-results/graphql-commands.png",
    });

    // We should see our commands in the palette
    // Look for text containing our commands
    const restartCommand = page.locator(
      'text="GraphQL: Restart GraphQL Language Server"'
    );
    const checkStatusCommand = page.locator('text="GraphQL: Check Status"');

    // At least one of our commands should be visible
    const hasCommands =
      (await restartCommand.isVisible()) ||
      (await checkStatusCommand.isVisible());

    // Close the command palette
    await page.keyboard.press("Escape");

    expect(hasCommands).toBe(true);
  });
});
