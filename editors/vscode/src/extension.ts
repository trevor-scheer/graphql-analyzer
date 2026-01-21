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
  TextEditor,
  TextEditorDecorationType,
  TextDocumentContentProvider,
  EventEmitter,
  Event,
  CancellationToken,
  Uri,
  Position,
  Range,
  env,
} from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Executable,
  State,
  Location as LspLocation,
  Position as LspPosition,
} from "vscode-languageclient/node";
import { findServerBinary } from "./binaryManager";

// =============================================================================
// LSP Command Arguments: Why Custom Commands Are Required
// =============================================================================
//
// VSCode's built-in commands like `editor.action.showReferences` expect native
// VSCode types (Uri, Position, Location) with actual methods on them. However,
// LSP servers send JSON which produces plain objects without methods.
//
// The vscode-languageclient library does NOT auto-convert command arguments -
// it only converts request/response payloads. This is a known limitation
// confirmed by the vscode-languageserver-node maintainer:
// https://github.com/microsoft/vscode-languageserver-node/issues/778
//
// Therefore, any LSP feature that needs to invoke VSCode commands with complex
// types (like CodeLens → showReferences) MUST use a custom command wrapper that:
// 1. Receives JSON arguments from the LSP server
// 2. Converts them to native VSCode types using protocol2CodeConverter
// 3. Calls the actual VSCode command
//
// This is the same pattern used by rust-analyzer and other mature LSP implementations.
// See: https://github.com/rust-lang/rust-analyzer/blob/master/editors/code/src/commands.ts
// =============================================================================

console.log(">>> GraphQL LSP extension imports complete <<<");

// =============================================================================
// Virtual File Support for Remote Schemas
// =============================================================================
//
// When goto definition navigates to a type in a remote schema (fetched via
// introspection), the LSP returns a URI like `schema://api.example.com/graphql/schema.graphql`.
// VSCode doesn't know how to open these URIs by default.
//
// We register a TextDocumentContentProvider for the "schema" scheme that:
// 1. Intercepts VSCode's attempts to open schema:// URIs
// 2. Fetches the schema content from the LSP server via a custom request
// 3. Returns the SDL content to display as a read-only document
//
// This allows users to navigate into remote schemas just like local files.
// =============================================================================

/**
 * Content provider for virtual files with the "schema" scheme.
 * Fetches schema content from the LSP server for display in VSCode.
 */
class SchemaContentProvider implements TextDocumentContentProvider {
  private _onDidChange = new EventEmitter<Uri>();

  get onDidChange(): Event<Uri> {
    return this._onDidChange.event;
  }

  async provideTextDocumentContent(uri: Uri, _token: CancellationToken): Promise<string> {
    if (!client) {
      return "// GraphQL LSP is not running";
    }

    try {
      // Request the virtual file content from the LSP server
      const content = await client.sendRequest<string | null>("graphql/virtualFileContent", {
        uri: uri.toString(),
      });

      if (content) {
        return content;
      }

      return `// Schema not found: ${uri.toString()}`;
    } catch (error) {
      return `// Error loading schema: ${error}`;
    }
  }
}

// Decoration type for deprecated GraphQL fields (strikethrough)
const deprecatedDecorationType: TextEditorDecorationType = window.createTextEditorDecorationType({
  textDecoration: "line-through",
  opacity: "0.7",
});

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

// Languages that can contain embedded GraphQL
const embeddedGraphQLLanguages = ["typescript", "typescriptreact", "javascript", "javascriptreact"];

async function updateDeprecatedDecorations(editor: TextEditor): Promise<void> {
  if (!client) {
    return;
  }

  const document = editor.document;
  const languageId = document.languageId;

  // Only process files that can contain embedded GraphQL
  if (!embeddedGraphQLLanguages.includes(languageId)) {
    return;
  }

  try {
    // Request semantic tokens from our LSP server
    const result = await client.sendRequest<{
      data?: number[];
    } | null>("textDocument/semanticTokens/full", {
      textDocument: { uri: document.uri.toString() },
    });

    if (!result || !result.data || result.data.length === 0) {
      // Clear decorations if no tokens
      editor.setDecorations(deprecatedDecorationType, []);
      return;
    }

    // Parse the delta-encoded tokens to find deprecated ones
    // Format: [deltaLine, deltaStart, length, tokenType, modifiers, ...]
    const deprecatedRanges: Range[] = [];
    let currentLine = 0;
    let currentChar = 0;

    for (let i = 0; i < result.data.length; i += 5) {
      const deltaLine = result.data[i];
      const deltaStart = result.data[i + 1];
      const length = result.data[i + 2];
      // const tokenType = result.data[i + 3]; // Not needed for this
      const modifiers = result.data[i + 4];

      // Update position
      if (deltaLine > 0) {
        currentLine += deltaLine;
        currentChar = deltaStart;
      } else {
        currentChar += deltaStart;
      }

      // Check if deprecated modifier is set (bit 0)
      const isDeprecated = (modifiers & 1) !== 0;

      if (isDeprecated) {
        const startPos = new Position(currentLine, currentChar);
        const endPos = new Position(currentLine, currentChar + length);
        deprecatedRanges.push(new Range(startPos, endPos));
      }
    }

    editor.setDecorations(deprecatedDecorationType, deprecatedRanges);
  } catch {
    // Silently ignore errors - the file may not be in a GraphQL project
  }
}

function setupDecorationListeners(context: ExtensionContext): void {
  // Update decorations when active editor changes
  context.subscriptions.push(
    window.onDidChangeActiveTextEditor((editor) => {
      if (editor) {
        updateDeprecatedDecorations(editor);
      }
    })
  );

  // Update decorations when document changes (debounced)
  let debounceTimer: NodeJS.Timeout | undefined;
  context.subscriptions.push(
    workspace.onDidChangeTextDocument((event) => {
      const editor = window.activeTextEditor;
      if (editor && editor.document === event.document) {
        if (debounceTimer) {
          clearTimeout(debounceTimer);
        }
        debounceTimer = setTimeout(() => updateDeprecatedDecorations(editor), 500);
      }
    })
  );

  // Update current editor immediately
  if (window.activeTextEditor) {
    updateDeprecatedDecorations(window.activeTextEditor);
  }
}

async function startLanguageServer(context: ExtensionContext): Promise<void> {
  const config = workspace.getConfiguration("graphql");
  const customPath = config.get<string>("server.path");
  const logLevel = config.get<string>("server.logLevel") || "info";

  const serverBinary = await findServerBinary(context, outputChannel, customPath);
  outputChannel.appendLine(`Using GraphQL CLI at: ${serverBinary}`);
  outputChannel.appendLine(`Server command: ${serverBinary} lsp`);

  const serverEnv = config.get<Record<string, string>>("server.env") || {};

  const run: Executable = {
    command: serverBinary,
    args: ["lsp"],
    options: {
      env: {
        ...process.env,
        RUST_LOG: process.env.RUST_LOG || logLevel,
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
      // Virtual files for remote schemas (introspected)
      { scheme: "schema", language: "graphql" },
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

    // Register content provider for virtual files (remote schemas)
    const schemaProvider = new SchemaContentProvider();
    context.subscriptions.push(
      workspace.registerTextDocumentContentProvider("schema", schemaProvider)
    );
    outputChannel.appendLine("Registered schema:// content provider for remote schemas");

    // Setup decoration listeners for deprecated fields in TS/JS files
    setupDecorationListeners(context);

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

    // Command wrapper for CodeLens "show references" functionality.
    // The LSP server sends this command with JSON arguments; we convert them
    // to native VSCode types before calling editor.action.showReferences.
    // See the comment block at the top of this file for why this is necessary.
    const showReferencesCommand = commands.registerCommand(
      "graphql-lsp.showReferences",
      async (uriString: string, position: LspPosition, locations: LspLocation[]) => {
        if (!client) {
          return;
        }

        const converter = client.protocol2CodeConverter;
        await commands.executeCommand(
          "editor.action.showReferences",
          Uri.parse(uriString),
          converter.asPosition(position),
          locations.map((loc) => converter.asLocation(loc))
        );
      }
    );

    // Command for "Copy as cURL" code lens
    // Generates a cURL command from the operation and copies to clipboard
    const copyAsCurlCommand = commands.registerCommand(
      "graphql-lsp.copyAsCurl",
      async (
        _uriString: string,
        operationSource: string,
        operationName: string,
        operationType: string,
        endpoint: string
      ) => {
        // Get endpoint from config or prompt user
        let targetEndpoint = endpoint;
        if (!targetEndpoint) {
          const config = workspace.getConfiguration("graphql");
          targetEndpoint = config.get<string>("endpoint") || "";
        }

        if (!targetEndpoint) {
          targetEndpoint =
            (await window.showInputBox({
              prompt: "Enter the GraphQL endpoint URL",
              placeHolder: "https://api.example.com/graphql",
            })) || "";
        }

        if (!targetEndpoint) {
          window.showWarningMessage("No endpoint provided");
          return;
        }

        // Build the GraphQL request body
        const body = JSON.stringify({
          query: operationSource,
          operationName: operationName || undefined,
        });

        // Generate cURL command
        const curlCommand = `curl -X POST '${targetEndpoint}' \\
  -H 'Content-Type: application/json' \\
  -d '${body.replace(/'/g, "'\\''")}'`;

        // Copy to clipboard
        await env.clipboard.writeText(curlCommand);

        const displayName = operationName || `anonymous ${operationType}`;
        window.showInformationMessage(`Copied cURL command for "${displayName}" to clipboard`);
        outputChannel.appendLine(`Copied cURL command: ${curlCommand}`);
      }
    );

    // Command for "Run" code lens
    // Executes the operation against the configured endpoint
    const runOperationCommand = commands.registerCommand(
      "graphql-lsp.runOperation",
      async (
        _uriString: string,
        operationSource: string,
        operationName: string,
        operationType: string,
        endpoint: string
      ) => {
        if (!endpoint) {
          window.showWarningMessage(
            "No endpoint configured. Add a schema URL via introspection in your .graphqlrc.yaml"
          );
          return;
        }

        const displayName = operationName || `anonymous ${operationType}`;
        outputChannel.appendLine(`Running ${operationType}: ${displayName}`);
        outputChannel.show(true);

        try {
          // Build the GraphQL request body
          const body = JSON.stringify({
            query: operationSource,
            operationName: operationName || undefined,
          });

          // Execute the request
          const response = await fetch(endpoint, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body,
          });

          const result = (await response.json()) as { data?: unknown; errors?: unknown[] };

          // Display result in output channel
          outputChannel.appendLine("Response:");
          outputChannel.appendLine(JSON.stringify(result, null, 2));
          outputChannel.appendLine("");

          if (result.errors && result.errors.length > 0) {
            window.showWarningMessage(`${displayName} completed with errors. See Output for details.`);
          } else {
            window.showInformationMessage(`${displayName} executed successfully`);
          }
        } catch (error) {
          const errorMessage = `Failed to execute ${displayName}: ${error}`;
          outputChannel.appendLine(errorMessage);
          window.showErrorMessage(errorMessage);
        }
      }
    );

    context.subscriptions.push(
      reloadCommand,
      checkStatusCommand,
      showReferencesCommand,
      copyAsCurlCommand,
      runOperationCommand
    );
  } catch (error) {
    const errorMessage = `Failed to start GraphQL LSP: ${error}`;
    outputChannel.appendLine(errorMessage);
    outputChannel.show(true);
    window.showErrorMessage(errorMessage);
    statusBarItem.text = "$(error) GraphQL";
    statusBarItem.tooltip = `GraphQL LSP failed to start: ${error}`;
    // Don't throw - allow partial activation so restart command can still work
  }

  context.subscriptions.push(outputChannel);
  outputChannel.appendLine("Extension activated!");
  console.log("=== Extension activation complete ===");
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
