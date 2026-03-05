import { test as base, _electron as electron, ElectronApplication, Page } from "@playwright/test";
import { downloadAndUnzipVSCode } from "@vscode/test-electron";
import * as path from "path";
import * as fs from "fs";
import * as os from "os";

export interface VSCodeFixtures {
  vscode: {
    app: ElectronApplication;
    page: Page;
    workspaceDir: string;
  };
}

let vscodeExecutablePath: string | undefined;
let extensionDevelopmentPath: string;

async function getVSCodeExecutable(): Promise<string> {
  if (vscodeExecutablePath) {
    return vscodeExecutablePath;
  }

  console.log("Downloading VS Code...");
  vscodeExecutablePath = await downloadAndUnzipVSCode("stable");
  console.log(`VS Code downloaded to: ${vscodeExecutablePath}`);
  return vscodeExecutablePath;
}

function getElectronPath(vscodeExePath: string): string {
  // The downloadAndUnzipVSCode returns the path to the Code executable
  // We need to find the Electron binary based on platform
  if (process.platform === "darwin") {
    // macOS: /path/to/Visual Studio Code.app/Contents/MacOS/Electron
    const appPath = path.dirname(path.dirname(vscodeExePath));
    return path.join(appPath, "MacOS", "Electron");
  } else if (process.platform === "win32") {
    // Windows: same directory as Code.exe
    return vscodeExePath;
  } else {
    // Linux: the code binary itself is the electron binary
    return vscodeExePath;
  }
}

// --- Test Schema ---
// A richer schema that exercises hover, completions, goto-def, code lenses,
// inlay hints, diagnostics, and deprecated-field features.
const TEST_SCHEMA = `\
"""A user in the system"""
type User {
  id: ID!
  name: String!
  email: String!
  """The user's age in years"""
  age: Int
  posts: [Post!]!
  oldField: String @deprecated(reason: "Use name instead")
}

type Post {
  id: ID!
  title: String!
  body: String!
  author: User!
}

type Query {
  """Fetch the current user"""
  me: User
  """Look up a user by ID"""
  user(id: ID!): User
  """List all posts"""
  posts: [Post!]!
  hello: String
}
`;

// A valid query that uses fragments across files, provides hover/completion targets
const TEST_QUERY = `\
query GetUser {
  me {
    ...UserFields
  }
}

query ListPosts {
  posts {
    title
    author {
      name
    }
  }
}
`;

// Fragment definition in a separate file — enables cross-file goto-def and references
const TEST_FRAGMENTS = `\
fragment UserFields on User {
  id
  name
  email
  age
}

fragment PostFields on Post {
  id
  title
  body
}
`;

// A deliberately broken query for diagnostics testing
const TEST_INVALID_QUERY = `\
query Broken {
  me {
    nonExistentField
  }
}
`;

export const test = base.extend<VSCodeFixtures>({
  vscode: async (_, use) => {
    extensionDevelopmentPath = path.resolve(__dirname, "..");

    // Ensure the extension is compiled
    const outDir = path.join(extensionDevelopmentPath, "out");
    if (!fs.existsSync(path.join(outDir, "extension.js"))) {
      throw new Error("Extension not compiled. Run `npm run compile` first in editors/vscode");
    }

    const vscodeExePath = await getVSCodeExecutable();
    const electronPath = getElectronPath(vscodeExePath);

    // Create a temporary user data directory for isolation
    const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), "vscode-test-user-data-"));

    // Create a temporary extensions directory
    const extensionsDir = fs.mkdtempSync(path.join(os.tmpdir(), "vscode-test-extensions-"));

    // Create test workspace with GraphQL files
    const workspaceDir = fs.mkdtempSync(path.join(os.tmpdir(), "vscode-test-workspace-"));

    // Create a subdirectory for document files so the documents glob doesn't match schema.graphql
    const docsDir = path.join(workspaceDir, "operations");
    fs.mkdirSync(docsDir, { recursive: true });

    // Create a .graphqlrc.yaml so the extension activates
    fs.writeFileSync(
      path.join(workspaceDir, ".graphqlrc.yaml"),
      `schema: schema.graphql\ndocuments: "operations/**/*.graphql"\n`,
    );

    // Create settings.json to disable AI features and other prompts
    const userSettingsDir = path.join(userDataDir, "User");
    fs.mkdirSync(userSettingsDir, { recursive: true });
    fs.writeFileSync(
      path.join(userSettingsDir, "settings.json"),
      JSON.stringify(
        {
          "github.copilot.enable": { "*": false },
          "github.copilot.chat.welcomeMessage": "never",
          "chat.commandCenter.enabled": false,
          "workbench.welcomePage.walkthroughs.openOnInstall": false,
          "workbench.startupEditor": "none",
          "security.workspace.trust.enabled": false,
          "editor.inlayHints.enabled": "on",
        },
        null,
        2,
      ),
    );

    // Write all test fixture files. Documents go in operations/ to avoid matching schema.
    fs.writeFileSync(path.join(workspaceDir, "schema.graphql"), TEST_SCHEMA);
    fs.writeFileSync(path.join(docsDir, "query.graphql"), TEST_QUERY);
    fs.writeFileSync(path.join(docsDir, "fragments.graphql"), TEST_FRAGMENTS);
    fs.writeFileSync(path.join(docsDir, "invalid.graphql"), TEST_INVALID_QUERY);

    const app = await electron.launch({
      executablePath: electronPath,
      args: [
        // VS Code CLI args
        `--extensionDevelopmentPath=${extensionDevelopmentPath}`,
        `--user-data-dir=${userDataDir}`,
        `--extensions-dir=${extensionsDir}`,
        "--disable-extensions", // Disable other extensions
        `--enable-proposed-api=${extensionDevelopmentPath}`,
        "--skip-welcome",
        "--skip-release-notes",
        "--disable-workspace-trust",
        "--disable-telemetry",
        workspaceDir,
      ],
      env: {
        ...process.env,
        // Disable GPU for CI environments
        DISPLAY: process.env.DISPLAY || ":0",
      },
    });

    // Wait for VS Code to be ready
    const page = await app.firstWindow();
    await page.waitForLoadState("domcontentloaded");

    // Wait for VSCode UI to be fully loaded (activity bar visible)
    const activityBar = page.locator(".activitybar");
    await activityBar.waitFor({ state: "visible", timeout: 30000 });

    // Open a GraphQL file to trigger extension activation via onLanguage:graphql
    const body = page.locator("body");
    const mod = process.platform === "darwin" ? "Meta" : "Control";
    await body.press(`${mod}+P`);

    const quickOpen = page.locator(".quick-input-widget");
    await quickOpen.waitFor({ state: "visible" });

    const input = quickOpen.locator("input");
    await input.fill("query.graphql");
    await input.press("Enter");

    // Wait for editor to open — use visible filter to skip hidden chat widget
    const editor = page.locator(".monaco-editor").locator("visible=true").first();
    await editor.waitFor({ state: "visible", timeout: 10000 });

    // Ensure quick-input is fully closed
    await input.press("Escape");

    // Click on editor to restore focus
    await editor.click();

    // Wait for LSP to be fully ready by polling the status bar.
    // The extension shows a checkmark icon when the server reports "ready".
    await waitForLspReady(page);

    await use({ app, page, workspaceDir });

    // Cleanup
    await app.close();

    // Clean up temp directories
    fs.rmSync(userDataDir, { recursive: true, force: true });
    fs.rmSync(extensionsDir, { recursive: true, force: true });
    fs.rmSync(workspaceDir, { recursive: true, force: true });
  },
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Wait for the LSP server to report "ready" via the status bar item.
 * Falls back to a timeout so tests don't hang forever.
 */
async function waitForLspReady(page: Page, timeoutMs = 30_000): Promise<void> {
  const start = Date.now();
  let debugLogged = false;
  while (Date.now() - start < timeoutMs) {
    // Check all status bar items for the graphql-analyzer text and tooltip
    const allItems = page.locator(".statusbar-item");
    const allCount = await allItems.count();

    for (let i = 0; i < allCount; i++) {
      const text = await allItems
        .nth(i)
        .textContent()
        .catch(() => "");
      if (text && text.includes("graphql-analyzer")) {
        const ariaLabel = await allItems
          .nth(i)
          .getAttribute("aria-label")
          .catch(() => null);
        if (!debugLogged) {
          console.log(`Status bar item found: text="${text}", aria-label="${ariaLabel}"`);
          debugLogged = true;
        }
        // Wait for background loading to complete. The status bar shows:
        // - During loading: "loading~spin graphql-analyzer" (aria: "Loading...")
        // - After ready: "check graphql-analyzer" (aria: "N files loaded in Xs")
        // The "files loaded" text confirms all project files have been indexed.
        if (ariaLabel && ariaLabel.includes("files loaded")) {
          console.log(`LSP ready: ${ariaLabel}`);
          return;
        }
      }
    }
    await page.waitForTimeout(500);
  }
  // Fallback: even if we can't detect readiness, give a generous pause
  console.warn("Could not detect LSP ready state via status bar, falling back to timeout");
  await page.waitForTimeout(5000);
}

/**
 * Open a file via the Quick Open dialog (Cmd/Ctrl+P).
 */
export async function openFile(page: Page, filename: string): Promise<void> {
  const mod = process.platform === "darwin" ? "Meta" : "Control";
  const body = page.locator("body");

  await body.press(`${mod}+P`);
  const quickOpen = page.locator(".quick-input-widget");
  await quickOpen.waitFor({ state: "visible" });

  const input = quickOpen.locator("input");
  await input.fill(filename);

  const fileResult = page.getByText(filename).first();
  await fileResult.waitFor({ state: "visible", timeout: 5000 });

  await input.press("Enter");

  // Wait for the editor to show the file
  const editorContent = page.locator(".view-lines").first();
  await editorContent.waitFor({ state: "visible", timeout: 5000 });

  // Dismiss any lingering quick-open
  await page.keyboard.press("Escape");
  // Small delay for the editor to settle
  await page.waitForTimeout(300);
}

/**
 * Run a VS Code command via the command palette.
 */
export async function runCommand(page: Page, command: string): Promise<void> {
  const mod = process.platform === "darwin" ? "Meta" : "Control";
  await page.locator("body").press(`${mod}+Shift+P`);
  const quickOpen = page.locator(".quick-input-widget");
  await quickOpen.waitFor({ state: "visible" });

  const input = quickOpen.locator("input");
  await input.fill(`>${command}`);

  const result = page.getByText(command).first();
  await result.waitFor({ state: "visible", timeout: 5000 });
  await input.press("Enter");

  // Wait for palette to close
  await quickOpen.waitFor({ state: "hidden", timeout: 3000 }).catch(() => {});
}

export { expect } from "@playwright/test";
