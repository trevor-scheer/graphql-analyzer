import {
  test as base,
  _electron as electron,
  ElectronApplication,
  Page,
} from "@playwright/test";
import {
  downloadAndUnzipVSCode,
  resolveCliArgsFromVSCodeExecutablePath,
} from "@vscode/test-electron";
import * as path from "path";
import * as fs from "fs";
import * as os from "os";

export interface VSCodeFixtures {
  vscode: {
    app: ElectronApplication;
    page: Page;
  };
}

let vscodeExecutablePath: string | undefined;
let extensionDevelopmentPath: string;

async function getVSCodeExecutable(): Promise<string> {
  if (vscodeExecutablePath) {
    return vscodeExecutablePath;
  }

  console.log("Downloading VS Code...");
  vscodeExecutablePath = await downloadAndUnzipVSCode("insiders");
  console.log(`VS Code downloaded to: ${vscodeExecutablePath}`);
  return vscodeExecutablePath;
}

function getElectronPath(vscodeExePath: string): string {
  // The downloadAndUnzipVSCode returns the path to the Code executable
  // We need to find the Electron binary based on platform
  if (process.platform === "darwin") {
    // macOS: /path/to/Visual Studio Code - Insiders.app/Contents/MacOS/Electron
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

export const test = base.extend<VSCodeFixtures>({
  vscode: async ({}, use) => {
    extensionDevelopmentPath = path.resolve(__dirname, "..");

    // Ensure the extension is compiled
    const outDir = path.join(extensionDevelopmentPath, "out");
    if (!fs.existsSync(path.join(outDir, "extension.js"))) {
      throw new Error(
        "Extension not compiled. Run `npm run compile` first in editors/vscode"
      );
    }

    const vscodeExePath = await getVSCodeExecutable();
    const electronPath = getElectronPath(vscodeExePath);

    // Create a temporary user data directory for isolation
    const userDataDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "vscode-test-user-data-")
    );

    // Create a temporary extensions directory
    const extensionsDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "vscode-test-extensions-")
    );

    // Create test workspace with GraphQL files
    const workspaceDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "vscode-test-workspace-")
    );

    // Create a .graphqlrc.yaml so the extension activates
    fs.writeFileSync(
      path.join(workspaceDir, ".graphqlrc.yaml"),
      `schema: schema.graphql\ndocuments: "**/*.graphql"\n`
    );

    // Create a simple schema file
    fs.writeFileSync(
      path.join(workspaceDir, "schema.graphql"),
      `type Query {\n  hello: String\n}\n`
    );

    // Create a simple query file
    fs.writeFileSync(
      path.join(workspaceDir, "query.graphql"),
      `query Hello {\n  hello\n}\n`
    );

    console.log(`Launching VS Code from: ${electronPath}`);
    console.log(`Extension path: ${extensionDevelopmentPath}`);
    console.log(`Workspace: ${workspaceDir}`);

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

    // Give VS Code a moment to initialize
    await page.waitForTimeout(2000);

    await use({ app, page });

    // Cleanup
    await app.close();

    // Clean up temp directories
    fs.rmSync(userDataDir, { recursive: true, force: true });
    fs.rmSync(extensionsDir, { recursive: true, force: true });
    fs.rmSync(workspaceDir, { recursive: true, force: true });
  },
});

export { expect } from "@playwright/test";
