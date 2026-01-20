import * as path from "path";
import * as fs from "fs";
import * as https from "https";
import { promisify } from "util";
import { exec } from "child_process";
import { ExtensionContext, window, OutputChannel, ProgressLocation } from "vscode";
import * as os from "os";

function expandTilde(inputPath: string): string {
  if (!inputPath) return inputPath;
  if (inputPath.startsWith("~")) {
    return path.join(os.homedir(), inputPath.slice(1));
  }
  return inputPath;
}

const execAsync = promisify(exec);

interface PlatformInfo {
  platform: string;
  arch: string;
  binaryName: string;
}

function getPlatformInfo(): PlatformInfo {
  const platform = process.platform;
  const arch = process.arch;

  let platformStr: string;
  let archStr: string;

  switch (platform) {
    case "darwin":
      platformStr = "apple-darwin";
      break;
    case "linux":
      platformStr = "unknown-linux-gnu";
      break;
    case "win32":
      platformStr = "pc-windows-msvc";
      break;
    default:
      throw new Error(`Unsupported platform: ${platform}`);
  }

  switch (arch) {
    case "x64":
      archStr = "x86_64";
      break;
    case "arm64":
      archStr = "aarch64";
      break;
    default:
      throw new Error(`Unsupported architecture: ${arch}`);
  }

  const binaryName = platform === "win32" ? "graphql.exe" : "graphql";

  return {
    platform: `${archStr}-${platformStr}`,
    arch: archStr,
    binaryName,
  };
}

async function findInPath(binaryName: string): Promise<string | null> {
  try {
    const cmd = process.platform === "win32" ? `where ${binaryName}` : `which ${binaryName}`;
    const { stdout } = await execAsync(cmd);
    const result = stdout.trim().split("\n")[0];
    return result || null;
  } catch {
    return null;
  }
}

async function downloadBinary(url: string, destination: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destination);
    https
      .get(url, (response) => {
        if (response.statusCode === 302 || response.statusCode === 301) {
          const redirectUrl = response.headers.location;
          if (redirectUrl) {
            https
              .get(redirectUrl, (redirectResponse) => {
                redirectResponse.pipe(file);
                file.on("finish", () => {
                  file.close();
                  resolve();
                });
              })
              .on("error", (err) => {
                fs.unlink(destination, () => {});
                reject(err);
              });
          } else {
            reject(new Error("Redirect location not found"));
          }
        } else {
          response.pipe(file);
          file.on("finish", () => {
            file.close();
            resolve();
          });
        }
      })
      .on("error", (err) => {
        fs.unlink(destination, () => {});
        reject(err);
      });
  });
}

async function extractTarXz(archivePath: string, extractDir: string): Promise<void> {
  const cmd =
    process.platform === "win32"
      ? `tar -xf "${archivePath}" -C "${extractDir}"`
      : `tar -xJf "${archivePath}" -C "${extractDir}"`;
  await execAsync(cmd);
}

async function extractZip(archivePath: string, extractDir: string): Promise<void> {
  const cmd =
    process.platform === "win32"
      ? `powershell -command "Expand-Archive -Path '${archivePath}' -DestinationPath '${extractDir}'"`
      : `unzip -q "${archivePath}" -d "${extractDir}"`;
  await execAsync(cmd);
}

/**
 * Ask user for permission to download the binary.
 * Returns true if user approves, false otherwise.
 */
async function askUserForDownloadPermission(): Promise<boolean> {
  const download = "Download";
  const cancel = "Cancel";

  const result = await window.showInformationMessage(
    "GraphQL CLI not found. Would you like to download it from GitHub releases?",
    { modal: true, detail: "You can also install manually with: cargo install graphql-cli" },
    download,
    cancel
  );

  return result === download;
}

async function downloadAndInstallBinary(
  context: ExtensionContext,
  platformInfo: PlatformInfo,
  outputChannel: OutputChannel
): Promise<string> {
  const storageDir = context.globalStorageUri.fsPath;
  if (!fs.existsSync(storageDir)) {
    fs.mkdirSync(storageDir, { recursive: true });
  }

  const binaryDir = path.join(storageDir, "bin");
  if (!fs.existsSync(binaryDir)) {
    fs.mkdirSync(binaryDir, { recursive: true });
  }

  const binaryPath = path.join(binaryDir, platformInfo.binaryName);

  if (fs.existsSync(binaryPath)) {
    outputChannel.appendLine(`Binary already exists at: ${binaryPath}`);
    return binaryPath;
  }

  outputChannel.appendLine("Fetching latest release information...");

  const releaseUrl = "https://api.github.com/repos/trevor-scheer/graphql-lsp/releases/latest";

  return new Promise((resolve, reject) => {
    https
      .get(
        releaseUrl,
        {
          headers: {
            "User-Agent": "vscode-graphql-lsp",
          },
        },
        (response) => {
          let data = "";
          response.on("data", (chunk) => {
            data += chunk;
          });
          response.on("end", async () => {
            try {
              const release = JSON.parse(data);
              const version = release.tag_name;

              outputChannel.appendLine(`Latest version: ${version}`);

              const isWindows = process.platform === "win32";
              const extension = isWindows ? "zip" : "tar.xz";
              const archiveName = `graphql-cli-${platformInfo.platform}.${extension}`;
              const downloadUrl = `https://github.com/trevor-scheer/graphql-lsp/releases/download/${version}/${archiveName}`;

              outputChannel.appendLine(`Downloading from: ${downloadUrl}`);

              const archivePath = path.join(storageDir, archiveName);

              await window.withProgress(
                {
                  location: ProgressLocation.Notification,
                  title: "Downloading GraphQL CLI...",
                  cancellable: false,
                },
                async () => {
                  await downloadBinary(downloadUrl, archivePath);
                }
              );

              outputChannel.appendLine("Download complete, extracting...");

              if (isWindows) {
                await extractZip(archivePath, storageDir);
              } else {
                await extractTarXz(archivePath, storageDir);
              }

              const extractedBinaryPath = path.join(
                storageDir,
                `graphql-cli-${platformInfo.platform}`,
                platformInfo.binaryName
              );

              if (fs.existsSync(extractedBinaryPath)) {
                fs.renameSync(extractedBinaryPath, binaryPath);
                const extractedDir = path.join(storageDir, `graphql-cli-${platformInfo.platform}`);
                fs.rmSync(extractedDir, { recursive: true, force: true });
              } else {
                throw new Error(`Binary not found after extraction at ${extractedBinaryPath}`);
              }

              if (!isWindows) {
                fs.chmodSync(binaryPath, 0o755);
              }

              fs.unlinkSync(archivePath);

              outputChannel.appendLine(`Binary installed successfully at: ${binaryPath}`);
              resolve(binaryPath);
            } catch (error) {
              reject(error);
            }
          });
        }
      )
      .on("error", reject);
  });
}

/**
 * Find the GraphQL CLI binary. The extension will use `graphql lsp` as the server command.
 *
 * Search order:
 * 1. Custom path from settings (graphql.server.path)
 * 2. GRAPHQL_PATH environment variable
 * 3. Development build (target/debug/graphql)
 * 4. System PATH
 * 5. Extension storage (previously downloaded)
 * 6. Download from GitHub releases (with user permission)
 */
export async function findServerBinary(
  context: ExtensionContext,
  outputChannel: OutputChannel,
  customPath?: string
): Promise<string> {
  const platformInfo = getPlatformInfo();

  // 1. Check custom path from settings
  if (customPath && customPath.trim() !== "") {
    const expandedCustomPath = expandTilde(customPath.trim());
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

  // 2. Check environment variable
  const envPathRaw = process.env.GRAPHQL_PATH;
  if (envPathRaw && envPathRaw.trim() !== "") {
    const envPath = expandTilde(envPathRaw.trim());
    outputChannel.appendLine(`Checking GRAPHQL_PATH: ${envPath}`);
    if (fs.existsSync(envPath)) {
      const stats = fs.statSync(envPath);
      if (stats.isDirectory()) {
        const binaryInDir = path.join(envPath, platformInfo.binaryName);
        if (fs.existsSync(binaryInDir)) {
          outputChannel.appendLine(`Found binary in GRAPHQL_PATH directory: ${binaryInDir}`);
          return binaryInDir;
        }
        outputChannel.appendLine(
          `GRAPHQL_PATH is a directory but binary not found: ${binaryInDir}`
        );
      } else {
        outputChannel.appendLine(`Found binary at GRAPHQL_PATH: ${envPath}`);
        return envPath;
      }
    } else {
      outputChannel.appendLine(`GRAPHQL_PATH does not exist: ${envPath}`);
    }
  }

  // 3. Check development path (for local development)
  const devPath = path.join(context.extensionPath, "../../target/debug/graphql");
  if (fs.existsSync(devPath)) {
    outputChannel.appendLine(`Found binary at dev path: ${devPath}`);
    return devPath;
  }

  // 4. Search PATH
  outputChannel.appendLine("Searching for graphql in PATH...");
  const pathBinary = await findInPath("graphql");
  if (pathBinary) {
    outputChannel.appendLine(`Found binary in PATH: ${pathBinary}`);
    return pathBinary;
  }

  // 5. Check extension storage for previously downloaded binary
  const storageDir = context.globalStorageUri.fsPath;
  const storedBinaryPath = path.join(storageDir, "bin", platformInfo.binaryName);
  if (fs.existsSync(storedBinaryPath)) {
    outputChannel.appendLine(`Found binary in storage: ${storedBinaryPath}`);
    return storedBinaryPath;
  }

  // 6. Ask user for permission to download
  outputChannel.appendLine("Binary not found. Asking user for download permission...");

  const userApproved = await askUserForDownloadPermission();
  if (!userApproved) {
    throw new Error("GraphQL CLI not found. Install it manually with: cargo install graphql-cli");
  }

  // 7. Download from GitHub releases
  outputChannel.appendLine("User approved download. Downloading from GitHub releases...");

  try {
    const downloadedPath = await downloadAndInstallBinary(context, platformInfo, outputChannel);
    return downloadedPath;
  } catch (error) {
    throw new Error(
      `Failed to download graphql binary. You can install it manually with: cargo install graphql-cli\n\nError: ${error}`
    );
  }
}
