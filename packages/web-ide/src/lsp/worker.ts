/// <reference lib="webworker" />
// Needed for DedicatedWorkerGlobalScope to be in scope for BrowserMessageReader/Writer.
import init, { Server } from "../wasm/graphql_lsp_wasm";
import { BrowserMessageReader, BrowserMessageWriter } from "vscode-jsonrpc/browser";

declare const self: DedicatedWorkerGlobalScope;

// Buffer messages that arrive before the wasm module finishes loading.
// BrowserMessageReader sets self.onmessage in its constructor, which fires any
// already-queued messages synchronously — before reader.listen() has registered
// a callback. We capture those messages here and replay them afterwards.
const pendingMessages: MessageEvent[] = [];
function bufferMessage(e: MessageEvent) {
  pendingMessages.push(e);
}
self.addEventListener("message", bufferMessage);

async function main() {
  await init();
  const server = new Server();

  // Stop buffering before BrowserMessageReader installs its own self.onmessage,
  // then replay any messages that arrived during wasm init.
  self.removeEventListener("message", bufferMessage);

  const reader = new BrowserMessageReader(self);
  const writer = new BrowserMessageWriter(self);

  reader.listen((msg) => {
    const json = JSON.stringify(msg);
    const outbound: string[] = server.handleMessage(json);
    for (const out of outbound) {
      writer.write(JSON.parse(out));
    }
  });

  // Replay buffered messages through the now-live reader path.
  for (const e of pendingMessages) {
    self.dispatchEvent(new MessageEvent("message", { data: e.data }));
  }
}

main().catch((e) => console.error("worker init failed", e));
