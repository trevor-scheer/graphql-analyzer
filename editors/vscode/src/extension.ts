console.log(">>> GraphQL LSP extension module loading <<<");

import {
  workspace,
  ExtensionContext,
  window,
  OutputChannel,
  ProgressLocation,
  commands,
} from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Executable,
} from "vscode-languageclient/node";
import { findServerBinary } from "./binaryManager";

console.log(">>> GraphQL LSP extension imports complete <<<");

let client: LanguageClient;
let outputChannel: OutputChannel;

async function startLanguageServer(context: ExtensionContext): Promise<void> {
  const config = workspace.getConfiguration("graphql-lsp");
  const customPath = config.get<string>("serverPath");

  const serverCommand = await findServerBinary(context, outputChannel, customPath);
  outputChannel.appendLine(`Using LSP server at: ${serverCommand}`);

  const run: Executable = {
    command: serverCommand,
    options: {
      env: {
        ...process.env,
        RUST_LOG: process.env.RUST_LOG || "debug",
      },
    },
  };

  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "graphql" },
      { scheme: "file", pattern: "**/*.{graphql,gql}" },
      { scheme: "file", language: "typescript" },
      { scheme: "file", language: "typescriptreact" },
      { scheme: "file", language: "javascript" },
      { scheme: "file", language: "javascriptreact" },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.{graphql,gql,ts,tsx,js,jsx}"),
    },
    outputChannel: outputChannel,
  };

  outputChannel.appendLine("Creating language client...");

  client = new LanguageClient(
    "graphql-lsp",
    "GraphQL Language Server",
    serverOptions,
    clientOptions
  );

  outputChannel.appendLine("Starting language client...");

  await window.withProgress(
    {
      location: ProgressLocation.Notification,
      title: "GraphQL LSP",
      cancellable: false,
    },
    async (progress) => {
      progress.report({ message: "Starting language server..." });

      await client.start();
      outputChannel.appendLine("Language client started successfully!");

      progress.report({ message: "Loading GraphQL configuration..." });

      await new Promise<void>((resolve) => {
        const disposable = client.onNotification("window/logMessage", (params) => {
          if (params.message === "GraphQL config loaded successfully") {
            window.showInformationMessage("GraphQL LSP: Configuration loaded successfully");
            disposable.dispose();
            resolve();
          }
        });

        setTimeout(() => {
          disposable.dispose();
          resolve();
        }, 5000);
      });
    }
  );
}

export async function activate(context: ExtensionContext) {
  outputChannel = window.createOutputChannel("GraphQL LSP Debug");
  outputChannel.show(true);
  outputChannel.appendLine("=== GraphQL LSP extension activating ===");

  try {
    await startLanguageServer(context);

    const reloadCommand = commands.registerCommand("graphql-lsp.restartServer", async () => {
      outputChannel.appendLine("=== Restarting GraphQL LSP ===");

      try {
        if (client) {
          outputChannel.appendLine("Stopping existing client...");
          await client.stop();
          outputChannel.appendLine("Client stopped");
        }

        await startLanguageServer(context);
        window.showInformationMessage("GraphQL LSP restarted successfully");
      } catch (error) {
        const errorMessage = `Failed to restart GraphQL LSP: ${error}`;
        outputChannel.appendLine(errorMessage);
        window.showErrorMessage(errorMessage);
      }
    });

    const checkStatusCommand = commands.registerCommand("graphql-lsp.checkStatus", async () => {
      outputChannel.appendLine("=== Checking GraphQL LSP Status ===");

      try {
        if (!client) {
          window.showWarningMessage("GraphQL LSP is not running");
          return;
        }

        // Show the output channel so users can see the detailed status
        outputChannel.show(true);

        await client.sendRequest("workspace/executeCommand", {
          command: "graphql.checkStatus",
          arguments: [],
        });
      } catch (error) {
        const errorMessage = `Failed to check status: ${error}`;
        outputChannel.appendLine(errorMessage);
        window.showErrorMessage(errorMessage);
      }
    });

    context.subscriptions.push(reloadCommand, checkStatusCommand);
  } catch (error) {
    const errorMessage = `Failed to start GraphQL LSP: ${error}`;
    outputChannel.appendLine(errorMessage);
    window.showErrorMessage(errorMessage);
    throw error;
  }

  outputChannel.appendLine("Extension activated!");
  console.log("=== Extension activation complete ===");
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
