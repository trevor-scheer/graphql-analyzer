import type { Linter } from "eslint";
import { gqlPluckFromCodeStringSync, parseCode } from "@graphql-tools/graphql-tag-pluck";
import { deleteEmbeddedRecord, getEmbeddedRecord, setEmbeddedRecord } from "./embedded";
import { EmbeddedSourceMap, mapLiteral } from "./source-map";
import { getPluckConfig } from "./services";

export const compatibilityProcessor = {
  supportsAutofix: true,
  preprocess(code: string, filename: string): Array<string | { text: string; filename: string }> {
    deleteEmbeddedRecord(filename);
    if (/\.(graphql|gql)$/.test(filename)) return [code];
    try {
      const options = { skipIndent: true, ...getPluckConfig(filename) };
      const container = /\.(vue|svelte|astro|gts|gjs)$/.test(filename);
      const blocks = container
        ? gqlPluckFromCodeStringSync(filename, code, options).map(({ body }) => {
            const start = code.indexOf(body);
            if (start < 0 || code.indexOf(body, start + 1) !== -1) {
              throw new Error(
                "Cannot uniquely map transformed component GraphQL to its source. Extract the document to a .graphql file.",
              );
            }
            const starts = Array.from({ length: body.length }, (_, index) => start + index);
            const templateLiteral = code[start - 1] === "`" && code[start + body.length] === "`";
            return {
              text: body,
              map: new EmbeddedSourceMap(
                body,
                starts,
                starts.map((offset) => offset + 1),
                start,
                start + body.length,
                templateLiteral,
              ),
            };
          })
        : parseCode({ code, filePath: filename, options }).map(({ content, start, end }) => ({
            text: content,
            map: mapLiteral(code, start + 1, end - 1, content),
          }));
      setEmbeddedRecord(filename, code, blocks);
      return [...blocks.map(({ text }) => ({ text, filename: "document.graphql" })), code];
    } catch (error) {
      deleteEmbeddedRecord(filename);
      // The host parser supplies its own syntax diagnostics for malformed source.
      if (error instanceof SyntaxError && "loc" in error) return [code];
      throw new Error(
        `[@graphql-analyzer/eslint-plugin] Cannot extract GraphQL from ${filename}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  },

  postprocess(messages: Linter.LintMessage[][], filename: string): Linter.LintMessage[] {
    const record = getEmbeddedRecord(filename);
    try {
      return messages
        .flatMap((group, index) => {
          const block = record?.blocks[index];
          return block
            ? group.map((message) => block.map.mapMessage(message, record!.lines))
            : group;
        })
        .sort((left, right) => left.line - right.line || left.column - right.column);
    } finally {
      deleteEmbeddedRecord(filename);
    }
  },
};
