import * as path from "path";
import * as fs from "fs";
import * as os from "os";
import { ExtensionContext, OutputChannel, workspace } from "vscode";

function expandPath(inputPath: string): string {
  if (!inputPath) return inputPath;
  // Expand ~ to home directory
  if (inputPath.startsWith("~")) {
    return path.join(os.homedir(), inputPath.slice(1));
  }
  // Resolve relative paths against workspace folder
  if (!path.isAbsolute(inputPath)) {
    const workspaceFolder = workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (workspaceFolder) {
      return path.join(workspaceFolder, inputPath);
    }
  }
  return inputPath;
}

interface PlatformInfo {
  binaryName: string;
}

function getPlatformInfo(): PlatformInfo {
  const platform = process.platform;
  const binaryName = platform === "win32" ? "graphql-lsp.exe" : "graphql-lsp";
  return { binaryName };
}

/**
 * Find the GraphQL LSP server binary.
 *
 * Search order:
 * 1. Custom path from settings (graphql.server.path)
 * 2. Development build (target/debug/graphql-lsp) - for local development
 * 3. Bundled binary - always available in platform-specific extension
 *
 * This extension uses platform-specific bundling: the correct binary for your
 * platform is included in the extension package. No downloads required.
 */
export async function findServerBinary(
  context: ExtensionContext,
  outputChannel: OutputChannel,
  customPath?: string,
): Promise<string> {
  const platformInfo = getPlatformInfo();

  // 1. Check custom path from settings
  if (customPath && customPath.trim() !== "") {
    const expandedCustomPath = expandPath(customPath.trim());
    outputChannel.appendLine(`Checking custom path: ${expandedCustomPath}`);
    if (fs.existsSync(expandedCustomPath)) {
      const stats = fs.statSync(expandedCustomPath);
      if (stats.isDirectory()) {
        const binaryInDir = path.join(expandedCustomPath, platformInfo.binaryName);
        if (fs.existsSync(binaryInDir)) {
          outputChannel.appendLine(`Found binary in custom directory: ${binaryInDir}`);
          return binaryInDir;
        }
        outputChannel.appendLine(`Custom path is a directory but binary not found: ${binaryInDir}`);
      } else {
        outputChannel.appendLine(`Found binary at custom path: ${expandedCustomPath}`);
        return expandedCustomPath;
      }
    } else {
      outputChannel.appendLine(`Custom path does not exist: ${expandedCustomPath}`);
    }
  }

  // 2. Check development path (for local development)
  // When running from the repo, use the debug build
  const devPath = path.join(context.extensionPath, "../../target/debug", platformInfo.binaryName);
  if (fs.existsSync(devPath)) {
    outputChannel.appendLine(`Found binary at dev path: ${devPath}`);
    return devPath;
  }

  // 3. Use bundled binary (platform-specific extension includes the binary)
  const bundledPath = path.join(context.extensionPath, "bin", platformInfo.binaryName);
  if (fs.existsSync(bundledPath)) {
    outputChannel.appendLine(`Using bundled binary: ${bundledPath}`);
    return bundledPath;
  }

  // This should not happen in production - the binary should always be bundled
  throw new Error(
    `GraphQL LSP binary not found. Expected bundled binary at: ${bundledPath}\n\n` +
      "This indicates a packaging issue with the extension. Please report this at:\n" +
      "https://github.com/trevor-scheer/graphql-analyzer/issues",
  );
}
