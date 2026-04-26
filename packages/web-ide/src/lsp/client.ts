import {
  BrowserMessageReader,
  BrowserMessageWriter,
  createMessageConnection,
} from "vscode-jsonrpc/browser";
import type { MessageConnection } from "vscode-jsonrpc/browser";
import type {
  InitializeParams,
  InitializeResult,
  PublishDiagnosticsParams,
  CompletionParams,
  CompletionList,
  CompletionItem,
  HoverParams,
  Hover,
  DidOpenTextDocumentParams,
  DidChangeTextDocumentParams,
} from "vscode-languageserver-protocol";

export type DiagnosticsListener = (params: PublishDiagnosticsParams) => void;

export class MinimalClient {
  private conn!: MessageConnection;
  private diagnosticsListeners: DiagnosticsListener[] = [];

  constructor(private worker: Worker) {}

  async start(initOptions: unknown): Promise<InitializeResult> {
    const reader = new BrowserMessageReader(this.worker);
    const writer = new BrowserMessageWriter(this.worker);
    this.conn = createMessageConnection(reader, writer);

    this.conn.onNotification(
      "textDocument/publishDiagnostics",
      (params: PublishDiagnosticsParams) => {
        for (const l of this.diagnosticsListeners) l(params);
      },
    );

    this.conn.listen();

    const params: InitializeParams = {
      processId: null,
      rootUri: null,
      capabilities: {
        textDocument: {
          synchronization: { dynamicRegistration: false },
          completion: { completionItem: { snippetSupport: false } },
          hover: { contentFormat: ["plaintext", "markdown"] },
          publishDiagnostics: {},
        },
      },
      initializationOptions: initOptions,
      workspaceFolders: null,
    };
    const result: InitializeResult = await this.conn.sendRequest("initialize", params);
    await this.conn.sendNotification("initialized", {});
    return result;
  }

  onDiagnostics(l: DiagnosticsListener): void {
    this.diagnosticsListeners.push(l);
  }

  didOpen(params: DidOpenTextDocumentParams): void {
    this.conn.sendNotification("textDocument/didOpen", params);
  }

  didChange(params: DidChangeTextDocumentParams): void {
    this.conn.sendNotification("textDocument/didChange", params);
  }

  completion(params: CompletionParams): Promise<CompletionList | CompletionItem[] | null> {
    return this.conn.sendRequest("textDocument/completion", params);
  }

  hover(params: HoverParams): Promise<Hover | null> {
    return this.conn.sendRequest("textDocument/hover", params);
  }
}
