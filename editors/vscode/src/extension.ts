console.log(">>> graphql-analyzer extension module loading <<<");

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
  ThemeColor,
} from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Executable,
  State,
  Trace,
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

console.log(">>> graphql-analyzer extension imports complete <<<");

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
      return "// graphql-analyzer is not running";
    }

    try {
      // Request the virtual file content from the LSP server
      const content = await client.sendRequest<string | null>(
        "graphql-analyzer/virtualFileContent",
        {
          uri: uri.toString(),
        },
      );

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
let traceOutputChannel: OutputChannel;
let statusBarItem: StatusBarItem;
let healthCheckInterval: NodeJS.Timeout | undefined;
let isServerHealthy = true;

function updateStatusBar(state: State): void {
  // Don't override unhealthy state unless the server is stopped/starting
  if (!isServerHealthy && state === State.Running) {
    return;
  }

  switch (state) {
    case State.Running:
      statusBarItem.text = "$(check) graphql-analyzer";
      statusBarItem.tooltip = "graphql-analyzer is running";
      statusBarItem.backgroundColor = undefined;
      isServerHealthy = true;
      break;
    case State.Starting:
      statusBarItem.text = "$(loading~spin) graphql-analyzer";
      statusBarItem.tooltip = "graphql-analyzer is starting...";
      statusBarItem.backgroundColor = new ThemeColor("statusBarItem.warningBackground");
      isServerHealthy = true;
      break;
    case State.Stopped:
      statusBarItem.text = "$(warning) graphql-analyzer";
      statusBarItem.tooltip = "graphql-analyzer is stopped";
      statusBarItem.backgroundColor = undefined;
      isServerHealthy = true;
      break;
  }
}

function setServerUnhealthy(reason: string): void {
  isServerHealthy = false;
  statusBarItem.text = "$(error) graphql-analyzer";
  statusBarItem.tooltip = `graphql-analyzer is unresponsive: ${reason}`;
  statusBarItem.backgroundColor = new ThemeColor("statusBarItem.errorBackground");
  outputChannel.appendLine(`[Health Check] Server unresponsive: ${reason}`);
}

function setServerHealthy(): void {
  if (!isServerHealthy) {
    isServerHealthy = true;
    statusBarItem.text = "$(check) graphql-analyzer";
    statusBarItem.tooltip = "graphql-analyzer is running";
    statusBarItem.backgroundColor = undefined;
    outputChannel.appendLine("[Health Check] Server recovered");
  }
}

async function performHealthCheck(timeout: number): Promise<void> {
  if (!client || client.state !== State.Running) {
    return;
  }

  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout);

    const pingPromise = client.sendRequest<{ timestamp: number }>("graphql-analyzer/ping");

    // Race the ping against the timeout
    const result = await Promise.race([
      pingPromise,
      new Promise<null>((_, reject) => {
        controller.signal.addEventListener("abort", () => {
          reject(new Error("Health check timed out"));
        });
      }),
    ]);

    clearTimeout(timeoutId);

    if (result) {
      setServerHealthy();
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setServerUnhealthy(message);
  }
}

function startHealthCheck(): void {
  stopHealthCheck();

  const config = workspace.getConfiguration("graphql");
  const enabled = config.get<boolean>("debug.healthCheck.enabled", false);

  if (!enabled) {
    outputChannel.appendLine("[Health Check] Disabled by configuration");
    return;
  }

  const interval = Math.max(5000, config.get<number>("debug.healthCheck.interval", 30000));
  const timeout = Math.max(1000, config.get<number>("debug.healthCheck.timeout", 5000));

  outputChannel.appendLine(
    `[Health Check] Starting with interval=${interval}ms, timeout=${timeout}ms`,
  );

  healthCheckInterval = setInterval(() => {
    performHealthCheck(timeout);
  }, interval);
}

function stopHealthCheck(): void {
  if (healthCheckInterval) {
    clearInterval(healthCheckInterval);
    healthCheckInterval = undefined;
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
    }),
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
    }),
  );

  // Update current editor immediately
  if (window.activeTextEditor) {
    updateDeprecatedDecorations(window.activeTextEditor);
  }
}

function syncTraceLevel(): void {
  if (!client) {
    return;
  }
  const enabled = workspace.getConfiguration("graphql").get<boolean>("trace.server", true);
  client.setTrace(enabled ? Trace.Verbose : Trace.Off);
}

async function startLanguageServer(context: ExtensionContext): Promise<void> {
  const config = workspace.getConfiguration("graphql");
  const customPath = config.get<string>("server.path");
  const logLevel = config.get<string>("server.logLevel") || "info";

  const serverBinary = await findServerBinary(context, outputChannel, customPath);
  outputChannel.appendLine(`Using GraphQL LSP server: ${serverBinary}`);

  const serverEnv = config.get<Record<string, string>>("server.env") || {};

  const run: Executable = {
    command: serverBinary,
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
    traceOutputChannel: traceOutputChannel,
  };

  outputChannel.appendLine("Creating language client...");

  client = new LanguageClient(
    "graphql-analyzer",
    "graphql-analyzer Language Server",
    serverOptions,
    clientOptions,
  );

  client.onDidChangeState((event) => {
    updateStatusBar(event.newState);
  });

  outputChannel.appendLine("Starting language client...");

  await window.withProgress(
    {
      location: ProgressLocation.Notification,
      title: "graphql-analyzer",
      cancellable: false,
    },
    async (progress) => {
      progress.report({ message: "Starting language server..." });

      await client.start();
      syncTraceLevel();
      outputChannel.appendLine("Language client started successfully!");

      client.onNotification(
        "graphql-analyzer/status",
        (params: { status: string; message?: string }) => {
          switch (params.status) {
            case "loading":
              statusBarItem.text = "$(loading~spin) graphql-analyzer";
              statusBarItem.backgroundColor = new ThemeColor("statusBarItem.warningBackground");
              statusBarItem.tooltip = params.message || "Loading GraphQL project...";
              break;
            case "ready":
              statusBarItem.text = "$(check) graphql-analyzer";
              statusBarItem.backgroundColor = undefined;
              statusBarItem.tooltip = params.message || "graphql-analyzer is running";
              isServerHealthy = true;
              // Start health check only after background initialization completes
              startHealthCheck();
              break;
          }
        },
      );
    },
  );
}

export async function activate(context: ExtensionContext) {
  outputChannel = window.createOutputChannel("graphql-analyzer Debug");
  traceOutputChannel = window.createOutputChannel("graphql-analyzer LSP");
  outputChannel.appendLine("=== graphql-analyzer extension activating ===");

  statusBarItem = window.createStatusBarItem(StatusBarAlignment.Right, 100);
  statusBarItem.command = "graphql-analyzer.checkStatus";
  statusBarItem.text = "$(sync~spin) graphql-analyzer";
  statusBarItem.tooltip = "graphql-analyzer is starting...";
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  try {
    await startLanguageServer(context);

    // Register content provider for virtual files (remote schemas)
    const schemaProvider = new SchemaContentProvider();
    context.subscriptions.push(
      workspace.registerTextDocumentContentProvider("schema", schemaProvider),
    );
    outputChannel.appendLine("Registered schema:// content provider for remote schemas");

    // Setup decoration listeners for deprecated fields in TS/JS files
    setupDecorationListeners(context);

    const reloadCommand = commands.registerCommand("graphql-analyzer.restartServer", async () => {
      outputChannel.appendLine("=== Restarting graphql-analyzer ===");

      try {
        stopHealthCheck();

        if (client) {
          outputChannel.appendLine("Stopping existing client...");
          await client.stop();
          outputChannel.appendLine("Client stopped");
        }

        await startLanguageServer(context);
        window.showInformationMessage("graphql-analyzer restarted successfully");
      } catch (error) {
        const errorMessage = `Failed to restart graphql-analyzer: ${error}`;
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
      "graphql-analyzer.showReferences",
      async (uriString: string, position: LspPosition, locations: LspLocation[]) => {
        if (!client) {
          return;
        }

        const converter = client.protocol2CodeConverter;
        await commands.executeCommand(
          "editor.action.showReferences",
          Uri.parse(uriString),
          converter.asPosition(position),
          locations.map((loc) => converter.asLocation(loc)),
        );
      },
    );

    // Listen for configuration changes
    context.subscriptions.push(
      workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration("graphql.trace.server")) {
          syncTraceLevel();
        }
        if (event.affectsConfiguration("graphql.debug.healthCheck")) {
          outputChannel.appendLine("[Health Check] Configuration changed, restarting...");
          startHealthCheck();
        }
      }),
    );

    context.subscriptions.push(reloadCommand, showReferencesCommand);
  } catch (error) {
    const errorMessage = `Failed to start graphql-analyzer: ${error}`;
    outputChannel.appendLine(errorMessage);
    outputChannel.show(true);
    window.showErrorMessage(errorMessage);
    statusBarItem.text = "$(error) graphql-analyzer";
    statusBarItem.tooltip = `graphql-analyzer failed to start: ${error}`;
    // Don't throw - allow partial activation so restart command can still work
  }

  context.subscriptions.push(outputChannel, traceOutputChannel);
  outputChannel.appendLine("Extension activated!");
  console.log("=== Extension activation complete ===");
}

export function deactivate(): Thenable<void> | undefined {
  stopHealthCheck();
  if (!client) {
    return undefined;
  }
  return client.stop();
}
