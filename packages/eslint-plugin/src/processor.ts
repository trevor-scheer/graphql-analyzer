import * as path from "path";
import * as binding from "./binding";

const JS_EXTENSIONS = new Set([".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"]);

interface BlockInfo {
  offset: number;
  lineOffset: number;
  columnOffset: number;
}

const extractionMap = new Map<string, BlockInfo[]>();

export const processor = {
  preprocess(code: string, filename: string): Array<string | { text: string; filename: string }> {
    const ext = path.extname(filename);
    if (!JS_EXTENSIONS.has(ext)) {
      return [code];
    }

    let blocks: binding.JsExtractedBlock[];
    try {
      blocks = binding.extractGraphql(code, ext.replace(".", ""));
    } catch {
      return [code];
    }

    if (blocks.length === 0) {
      return [code];
    }

    const blockInfos: BlockInfo[] = blocks.map((b) => {
      const prefix = code.slice(0, b.offset);
      const lines = prefix.split("\n");
      return {
        offset: b.offset,
        lineOffset: lines.length - 1,
        columnOffset: lines[lines.length - 1].length,
      };
    });
    extractionMap.set(filename, blockInfos);

    return [
      ...blocks.map((b, i) => ({
        text: b.source,
        filename: `${i}.graphql`,
      })),
      code,
    ];
  },

  postprocess(messages: any[][], _filename: string): any[] {
    return messages.flat();
  },

  supportsAutofix: true,
};
