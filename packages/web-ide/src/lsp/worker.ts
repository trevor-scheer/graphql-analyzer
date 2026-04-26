/// <reference lib="webworker" />
// Needed for DedicatedWorkerGlobalScope to be in scope for BrowserMessageReader/Writer.
import init, { Server } from "../wasm/graphql_lsp_wasm";
import {
  BrowserMessageReader,
  BrowserMessageWriter,
} from "vscode-jsonrpc/browser";

declare const self: DedicatedWorkerGlobalScope;

async function main() {
  await init();
  const server = new Server();

  const reader = new BrowserMessageReader(self);
  const writer = new BrowserMessageWriter(self);

  reader.listen((msg) => {
    const json = JSON.stringify(msg);
    const outbound: string[] = server.handleMessage(json);
    for (const out of outbound) {
      writer.write(JSON.parse(out));
    }
  });
}

main().catch((e) => console.error("worker init failed", e));
