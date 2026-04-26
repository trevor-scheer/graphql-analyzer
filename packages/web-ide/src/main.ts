import * as monaco from "monaco-editor";
import { MinimalClient } from "./lsp/client";
import { wireProviders } from "./lsp/providers";

// Expose monaco globally so Playwright tests can call getModelMarkers.
(window as unknown as Record<string, unknown>).__monaco = monaco;
import schemaText from "./demo/schema.graphql?raw";
import docText from "./demo/queries/example.graphql?raw";

monaco.languages.register({ id: "graphql", extensions: [".graphql", ".gql"] });

const schemaUri = monaco.Uri.parse("inmemory://schema.graphql");
const docUri = monaco.Uri.parse("inmemory://queries/example.graphql");

const schemaModel = monaco.editor.createModel(schemaText, "graphql", schemaUri);
const docModel = monaco.editor.createModel(docText, "graphql", docUri);

monaco.editor.create(document.getElementById("schema")!, {
  model: schemaModel,
  automaticLayout: true,
});
monaco.editor.create(document.getElementById("doc")!, {
  model: docModel,
  automaticLayout: true,
});

console.log("Creating LSP worker...");
const worker = new Worker(new URL("./lsp/worker.ts", import.meta.url), {
  type: "module",
});
worker.addEventListener("error", (e) => {
  console.error("LSP worker error:", e.message, e.filename, e.lineno);
});
worker.addEventListener("messageerror", (e) => {
  console.error("LSP worker messageerror:", e);
});
console.log("LSP worker created");
const client = new MinimalClient(worker);
const initOptions = {
  schema: "schema.graphql",
  documents: "**/*.graphql",
};
let initResult;
try {
  initResult = await client.start(initOptions);
  console.log("LSP initialized", initResult.capabilities);
  // Signal to e2e tests that the LSP is ready.
  (window as unknown as Record<string, unknown>).__lspReady = true;
} catch (e) {
  console.error("LSP initialize failed", e);
  throw e;
}

wireProviders(client);

function pushOpen(model: monaco.editor.ITextModel) {
  client.didOpen({
    textDocument: {
      uri: model.uri.toString(),
      languageId: "graphql",
      version: model.getVersionId(),
      text: model.getValue(),
    },
  });
  model.onDidChangeContent((e) => {
    client.didChange({
      textDocument: {
        uri: model.uri.toString(),
        version: model.getVersionId(),
      },
      contentChanges: e.changes.map((c) => ({
        range: {
          start: {
            line: c.range.startLineNumber - 1,
            character: c.range.startColumn - 1,
          },
          end: {
            line: c.range.endLineNumber - 1,
            character: c.range.endColumn - 1,
          },
        },
        rangeLength: c.rangeLength,
        text: c.text,
      })),
    });
  });
}
pushOpen(schemaModel);
pushOpen(docModel);
