import * as monaco from "monaco-editor";
import type { MinimalClient } from "./client";
import type {
  PublishDiagnosticsParams,
  Diagnostic,
  CompletionItem as LspCompletionItem,
  Hover as LspHover,
} from "vscode-languageserver-protocol";

const lspSeverityToMarker: Record<number, monaco.MarkerSeverity> = {
  1: monaco.MarkerSeverity.Error,
  2: monaco.MarkerSeverity.Warning,
  3: monaco.MarkerSeverity.Info,
  4: monaco.MarkerSeverity.Hint,
};

export function wireProviders(client: MinimalClient): void {
  monaco.languages.registerCompletionItemProvider("graphql", {
    triggerCharacters: [".", "$", "@", " ", "{", "(", ":"],
    provideCompletionItems: async (model, position) => {
      const params = {
        textDocument: { uri: model.uri.toString() },
        position: {
          line: position.lineNumber - 1,
          character: position.column - 1,
        },
      };
      const result = await client.completion(params);
      const items: LspCompletionItem[] = Array.isArray(result) ? result : (result?.items ?? []);
      const word = model.getWordUntilPosition(position);
      const range: monaco.IRange = {
        startLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endLineNumber: position.lineNumber,
        endColumn: word.endColumn,
      };
      return {
        suggestions: items.map((it) => ({
          label: it.label,
          kind: monaco.languages.CompletionItemKind.Text,
          insertText: typeof it.insertText === "string" ? it.insertText : it.label,
          detail: it.detail,
          documentation:
            typeof it.documentation === "string" ? it.documentation : it.documentation?.value,
          range,
        })),
      };
    },
  });

  monaco.languages.registerHoverProvider("graphql", {
    provideHover: async (model, position) => {
      const params = {
        textDocument: { uri: model.uri.toString() },
        position: {
          line: position.lineNumber - 1,
          character: position.column - 1,
        },
      };
      const hover: LspHover | null = await client.hover(params);
      if (!hover) return null;
      const contents = Array.isArray(hover.contents) ? hover.contents : [hover.contents];
      return {
        contents: contents.map((c) => (typeof c === "string" ? { value: c } : { value: c.value })),
      };
    },
  });

  client.onDiagnostics((params: PublishDiagnosticsParams) => {
    const model = monaco.editor.getModels().find((m) => m.uri.toString() === params.uri);
    if (!model) return;
    const markers = params.diagnostics.map((d: Diagnostic) => ({
      severity: lspSeverityToMarker[d.severity ?? 1] ?? monaco.MarkerSeverity.Error,
      startLineNumber: d.range.start.line + 1,
      startColumn: d.range.start.character + 1,
      endLineNumber: d.range.end.line + 1,
      endColumn: d.range.end.character + 1,
      message: d.message,
      source: d.source ?? "graphql",
    }));
    monaco.editor.setModelMarkers(model, "graphql", markers);
  });
}
