import * as monaco from "monaco-editor";
import { MinimalClient } from "./lsp/client";
import { wireProviders } from "./lsp/providers";
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

const worker = new Worker(new URL("./lsp/worker.ts", import.meta.url), {
  type: "module",
});
const client = new MinimalClient(worker);
const initOptions = {
  schema: "schema.graphql",
  documents: "**/*.graphql",
};
await client.start(initOptions);

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
