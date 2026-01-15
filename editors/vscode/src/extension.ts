console.log(">>> GraphQL LSP extension module loading <<<");

import {
  workspace,
  ExtensionContext,
  window,
  OutputChannel,
  ProgressLocation,
  commands,
  StatusBarItem,
  StatusBarAlignment,
} from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Executable,
  State,
} from "vscode-languageclient/node";
import { findServerBinary } from "./binaryManager";

console.log(">>> GraphQL LSP extension imports complete <<<");

let client: LanguageClient;
let outputChannel: OutputChannel;
let statusBarItem: StatusBarItem;

function updateStatusBar(state: State): void {
  switch (state) {
    case State.Running:
      statusBarItem.text = "$(check) GraphQL";
      statusBarItem.tooltip = "GraphQL LSP is running";
      statusBarItem.backgroundColor = undefined;
      break;
    case State.Starting:
      statusBarItem.text = "$(sync~spin) GraphQL";
      statusBarItem.tooltip = "GraphQL LSP is starting...";
      statusBarItem.backgroundColor = undefined;
      break;
    case State.Stopped:
      statusBarItem.text = "$(warning) GraphQL";
      statusBarItem.tooltip = "GraphQL LSP is stopped";
      statusBarItem.backgroundColor = undefined;
      break;
  }
}

async function startLanguageServer(context: ExtensionContext): Promise<void> {
  const config = workspace.getConfiguration("graphql");
  const customPath = config.get<string>("server.path");

  const serverCommand = await findServerBinary(context, outputChannel, customPath);
  outputChannel.appendLine(`Using LSP server at: ${serverCommand}`);

  const serverEnv = config.get<Record<string, string>>("server.env") || {};

  const run: Executable = {
    command: serverCommand,
    options: {
      env: {
        ...process.env,
        RUST_LOG: process.env.RUST_LOG || "debug",
        ...serverEnv,
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

  client.onDidChangeState((event) => {
    updateStatusBar(event.newState);
  });

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
    }
  );
}

export async function activate(context: ExtensionContext) {
  outputChannel = window.createOutputChannel("GraphQL LSP Debug");
  outputChannel.appendLine("=== GraphQL LSP extension activating ===");

  statusBarItem = window.createStatusBarItem(StatusBarAlignment.Right, 100);
  statusBarItem.command = "graphql-lsp.checkStatus";
  statusBarItem.text = "$(sync~spin) GraphQL";
  statusBarItem.tooltip = "GraphQL LSP is starting...";
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

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
        outputChannel.show(true);
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

        outputChannel.show(true);

        await client.sendRequest("workspace/executeCommand", {
          command: "graphql.checkStatus",
          arguments: [],
        });
      } catch (error) {
        const errorMessage = `Failed to check status: ${error}`;
        outputChannel.appendLine(errorMessage);
        outputChannel.show(true);
        window.showErrorMessage(errorMessage);
      }
    });

    context.subscriptions.push(reloadCommand, checkStatusCommand);
  } catch (error) {
    const errorMessage = `Failed to start GraphQL LSP: ${error}`;
    outputChannel.appendLine(errorMessage);
    outputChannel.show(true);
    window.showErrorMessage(errorMessage);
    statusBarItem.text = "$(error) GraphQL";
    statusBarItem.tooltip = `GraphQL LSP failed to start: ${error}`;
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
