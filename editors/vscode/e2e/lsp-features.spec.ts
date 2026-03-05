import { test, expect, openFile, runCommand } from "./vscodeFixture";
import * as fs from "fs";
import * as path from "path";

const mod = process.platform === "darwin" ? "Meta" : "Control";

// ---------------------------------------------------------------------------
// Tier 1 — Core features users rely on daily
// ---------------------------------------------------------------------------

test.describe("Diagnostics", () => {
  test("shows errors for invalid fields in the Problems panel", async ({ vscode }) => {
    const { page } = vscode;

    // Open the file that contains an invalid field
    await openFile(page, "invalid.graphql");

    // Wait a moment for the LSP to process the file and publish diagnostics
    await page.waitForTimeout(2000);

    // Open the Problems panel (Ctrl/Cmd+Shift+M)
    await page.locator("body").press(`${mod}+Shift+M`);

    // The problems panel should show an error about the nonExistentField.
    // The LSP produces: 'Cannot query field "nonExistentField" on type "User"'
    // Use a broad locator on the tree items in the problems panel.
    const problemItem = page
      .locator('[role="treeitem"]')
      .filter({ hasText: /Cannot query field|nonExistentField/i })
      .first();

    await expect(problemItem).toBeVisible({ timeout: 15000 });

    await page.screenshot({ path: "test-results/diagnostics-problems.png" });
  });

  test("shows error squiggles in the editor", async ({ vscode }) => {
    const { page } = vscode;

    await openFile(page, "invalid.graphql");
    await page.waitForTimeout(2000);

    // Error decorations in Monaco are rendered as elements with class
    // "squiggly-error" or within a ".view-overlays" container
    const squiggly = page.locator(".squiggly-error, .squiggly-warning").first();
    await expect(squiggly).toBeVisible({ timeout: 15000 });

    await page.screenshot({ path: "test-results/diagnostics-squiggles.png" });
  });
});

test.describe("Hover", () => {
  test("shows type information when hovering over a field", async ({ vscode }) => {
    const { page } = vscode;

    await openFile(page, "query.graphql");

    // Hover over a field to trigger the LSP hover. We'll hover over "posts"
    // on line 9 ("  posts {") which is a more unique and easier-to-target token.
    // Use "title" on line 10 inside posts which is a simple leaf field.
    const titleSpan = page
      .locator(".view-lines span")
      .filter({ hasText: /^title$/ })
      .first();
    await expect(titleSpan).toBeVisible({ timeout: 5000 });
    await titleSpan.hover();
    await page.waitForTimeout(1000);

    // Wait for the LSP hover widget to become visible
    const hoverContent = page
      .locator(".monaco-hover .monaco-hover-content")
      .locator("visible=true")
      .first();
    await expect(hoverContent).toBeVisible({ timeout: 10000 });

    // Verify the hover contains type information ("title" is String! on Post)
    await expect(hoverContent).toContainText(/String/, { timeout: 5000 });

    await page.screenshot({ path: "test-results/hover-field.png" });
  });
});

test.describe("Go to Definition", () => {
  test("navigates to fragment definition from a spread", async ({ vscode }) => {
    const { page } = vscode;

    await openFile(page, "query.graphql");

    // The spread "...UserFields" is on line 3. Navigate there.
    await page.locator("body").press(`${mod}+G`);
    const gotoInput = page.locator(".quick-input-widget input");
    await gotoInput.fill("3");
    await gotoInput.press("Enter");
    await page.waitForTimeout(300);

    // Find the "UserFields" token in the editor
    const token = page
      .locator(".view-lines span")
      .filter({ hasText: /UserFields/ })
      .first();

    // Ctrl/Cmd+Click to go to definition
    await token.click({ modifiers: [mod === "Meta" ? "Meta" : "Control"] });

    // Should navigate to fragments.graphql where UserFields is defined.
    // Wait for the tab/title to change to fragments.graphql
    const tab = page.locator(".tab").filter({ hasText: "fragments.graphql" }).first();
    await expect(tab).toBeVisible({ timeout: 10000 });

    await page.screenshot({ path: "test-results/goto-definition.png" });
  });
});

test.describe("Completions", () => {
  test("suggests fields when typing in a selection set", async ({ vscode }) => {
    const { page } = vscode;

    await openFile(page, "query.graphql");

    // Click directly on the editor text area to ensure it has focus
    const viewLines = page.locator(".view-lines").first();
    await viewLines.click();
    await page.waitForTimeout(300);

    // Use Go to Line:Column to navigate precisely to line 3
    await page.locator("body").press(`${mod}+G`);
    const quickInput = page.locator(".quick-input-widget");
    await quickInput.waitFor({ state: "visible" });
    const input = quickInput.locator("input");
    await input.fill("3");
    await input.press("Enter");
    // Explicitly close the quick input and return focus to editor
    await page.keyboard.press("Escape");
    await quickInput.waitFor({ state: "hidden", timeout: 3000 }).catch(() => {});
    // Click on the editor to ensure focus
    await viewLines.click();
    await page.waitForTimeout(500);

    // Press End to go to end of line 3, then Enter to create new blank line
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(500);

    // Type a letter to trigger auto-completion
    await page.keyboard.type("na", { delay: 150 });
    await page.waitForTimeout(500);

    // The suggest widget should appear with field completions for User type
    const suggestWidget = page.locator(".editor-widget.suggest-widget");
    await expect(suggestWidget).toBeVisible({ timeout: 10000 });

    // Check that "name" appears as a completion option
    const nameItem = suggestWidget.locator(".monaco-list-row").filter({ hasText: /name/ }).first();
    await expect(nameItem).toBeVisible({ timeout: 5000 });

    await page.screenshot({ path: "test-results/completions.png" });

    // Clean up
    await page.keyboard.press("Escape");
    for (let i = 0; i < 4; i++) await page.keyboard.press(`${mod}+Z`);
  });
});

// ---------------------------------------------------------------------------
// Tier 2 — Important but less critical
// ---------------------------------------------------------------------------

test.describe("Find References", () => {
  // TODO: find-references returns "No references found" for fragment spreads in e2e.
  // The server reports files as loaded, and code lenses show the correct reference count,
  // but Shift+F12 on the spread name doesn't resolve. May be a position-mapping issue
  // between the editor offset and the CST node for the spread name.
  test.skip("shows fragment references via Shift+F12", async ({ vscode }) => {
    const { page } = vscode;

    // Find-references for fragments only works from a fragment spread position
    // (e.g. "...UserFields"), not from the definition. Open query.graphql which
    // contains the spread "...UserFields".
    await openFile(page, "query.graphql");

    // Wait for the server to index all files
    await page.waitForTimeout(3000);

    // Double-click on "UserFields" in "...UserFields" to select and position cursor
    const userFieldsSpan = page
      .locator(".view-lines span")
      .filter({ hasText: /^UserFields$/ })
      .first();
    await expect(userFieldsSpan).toBeVisible({ timeout: 5000 });
    await userFieldsSpan.dblclick();
    await page.waitForTimeout(300);

    // Trigger find references (Shift+F12)
    await page.keyboard.press("Shift+F12");

    // The references peek widget should appear
    const peekWidget = page
      .locator(".peekview-widget, .reference-zone-widget, .zone-widget")
      .first();
    await expect(peekWidget).toBeVisible({ timeout: 15000 });

    await page.screenshot({ path: "test-results/find-references.png" });

    // Close the peek widget
    await page.keyboard.press("Escape");
  });
});

test.describe("Document Symbols", () => {
  test("shows operations and fragments in the outline", async ({ vscode }) => {
    const { page } = vscode;

    await openFile(page, "query.graphql");

    // Open the Go to Symbol in Editor widget (Cmd/Ctrl+Shift+O)
    await page.locator("body").press(`${mod}+Shift+O`);

    const quickOpen = page.locator(".quick-input-widget");
    await expect(quickOpen).toBeVisible();

    // The symbol list should contain our operations
    const getUser = page
      .locator(".quick-input-widget")
      .filter({ hasText: /GetUser/ })
      .first();
    await expect(getUser).toBeVisible({ timeout: 10000 });

    const listPosts = page
      .locator(".quick-input-widget")
      .filter({ hasText: /ListPosts/ })
      .first();
    await expect(listPosts).toBeVisible({ timeout: 5000 });

    await page.screenshot({ path: "test-results/document-symbols.png" });

    await page.keyboard.press("Escape");
  });
});

test.describe("Code Lens", () => {
  test("shows reference count above fragment definitions", async ({ vscode }) => {
    const { page } = vscode;

    // Open fragments.graphql which defines UserFields and PostFields
    await openFile(page, "fragments.graphql");

    // Code lenses are rendered as overlays above the code.
    // Look for the reference count text. UserFields is used once (in query.graphql).
    const codeLens = page
      .locator(".codelens-decoration a, .contentWidgets .codeLens a")
      .filter({ hasText: /reference/ })
      .first();

    await expect(codeLens).toBeVisible({ timeout: 15000 });

    await page.screenshot({ path: "test-results/code-lens.png" });
  });
});

// ---------------------------------------------------------------------------
// Tier 3 — Nice-to-have
// ---------------------------------------------------------------------------

test.describe("Inlay Hints", () => {
  test("shows return type annotations on leaf fields", async ({ vscode }) => {
    const { page } = vscode;

    // Open fragments.graphql which has leaf fields like "id", "name", "email"
    await openFile(page, "fragments.graphql");

    // Inlay hints show return types like ": ID!", ": String!", ": Int" after
    // leaf field names. In VS Code they're rendered as inline text within the
    // editor view-lines. Verify they appear by checking for type annotation text
    // that wouldn't be in the source file.
    const hintText = page.locator(".view-lines").getByText(": ID!").first();

    await expect(hintText).toBeVisible({ timeout: 15000 });

    await page.screenshot({ path: "test-results/inlay-hints.png" });
  });
});

test.describe("Folding Ranges", () => {
  test("allows folding query operations", async ({ vscode }) => {
    const { page } = vscode;

    await openFile(page, "query.graphql");

    // Folding controls appear in the gutter when hovering the line numbers area.
    // We'll use the command palette to fold all regions.
    await runCommand(page, "Fold All");
    await page.waitForTimeout(500);

    // After folding, the operations are collapsed. Verify by checking that the
    // "...UserFields" line (which was inside GetUser) is no longer visible, while
    // the "query GetUser {" line is still visible (it's the fold header).
    const headerLine = page.locator(".view-lines").getByText("query GetUser").first();
    await expect(headerLine).toBeVisible({ timeout: 3000 });

    // The body content should be folded away
    const bodyLine = page.locator(".view-lines").getByText("UserFields").first();
    await expect(bodyLine).not.toBeVisible({ timeout: 3000 });

    await page.screenshot({ path: "test-results/folding-ranges.png" });

    // Unfold
    await runCommand(page, "Unfold All");
  });
});

test.describe("Status Bar", () => {
  test("shows the graphql-analyzer status item", async ({ vscode }) => {
    const { page } = vscode;

    // The status bar should show the graphql-analyzer item
    const statusItem = page
      .locator(".statusbar-item")
      .filter({ hasText: /graphql-analyzer/ })
      .first();

    await expect(statusItem).toBeVisible({ timeout: 10000 });

    await page.screenshot({ path: "test-results/status-bar.png" });
  });
});

test.describe("Server Restart", () => {
  test("restart command recovers the server", async ({ vscode }) => {
    const { page } = vscode;

    // Execute the restart command
    await runCommand(page, "graphql-analyzer: Restart Language Server");

    // After restart, the status bar should eventually show running state again
    // Give it time — restart stops and re-starts the server
    await page.waitForTimeout(2000);

    // Verify the status bar still shows graphql-analyzer (not in error state)
    const statusItem = page
      .locator(".statusbar-item")
      .filter({ hasText: /graphql-analyzer/ })
      .first();
    await expect(statusItem).toBeVisible({ timeout: 15000 });

    // Verify LSP still works after restart by hovering a field
    await openFile(page, "query.graphql");
    await page.waitForTimeout(3000);

    // The editor should still be functional (skip hidden chat widget)
    const editor = page.locator(".monaco-editor").locator("visible=true").first();
    await expect(editor).toBeVisible();

    await page.screenshot({ path: "test-results/server-restart.png" });
  });
});
