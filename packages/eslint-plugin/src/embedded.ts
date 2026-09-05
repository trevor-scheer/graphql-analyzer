import * as path from "node:path";
import type { JsDiagnostic, JsTextEdit } from "./binding";
import { EmbeddedSourceMap, lineStarts, locationAt, offsetAt } from "./source-map";

export interface EmbeddedBlock {
  text: string;
  map: EmbeddedSourceMap;
}

export interface EmbeddedRecord {
  filename: string;
  source: string;
  lines: number[];
  blocks: EmbeddedBlock[];
  diagnostics?: JsDiagnostic[];
}

const records = new Map<string, EmbeddedRecord>();

export function setEmbeddedRecord(filename: string, source: string, blocks: EmbeddedBlock[]): void {
  const key = path.resolve(filename);
  const record = { filename, source, lines: lineStarts(source), blocks };
  records.set(key, record);
  // ESLint processes each file synchronously, but a throwing rule can skip postprocess.
  queueMicrotask(() => {
    if (records.get(key) === record) records.delete(key);
  });
}

export function getEmbeddedRecord(filename: string): EmbeddedRecord | undefined {
  return records.get(path.resolve(filename));
}

export function deleteEmbeddedRecord(filename: string): void {
  records.delete(path.resolve(filename));
}

export function findEmbeddedBlock(
  filename: string,
): { record: EmbeddedRecord; block: EmbeddedBlock } | undefined {
  const match = /[/\\](\d+)_document\.graphql$/.exec(filename);
  if (!match) return undefined;
  const record = getEmbeddedRecord(filename.slice(0, match.index));
  const block = record?.blocks[Number(match[1])];
  return record && block ? { record, block } : undefined;
}

export function embeddedDiagnostics(
  filename: string,
  lint: (filename: string, source: string) => JsDiagnostic[],
): JsDiagnostic[] | undefined {
  const embedded = findEmbeddedBlock(filename);
  const record = embedded?.record ?? getEmbeddedRecord(filename);
  if (!record) return undefined;
  record.diagnostics ??= lint(record.filename, record.source);
  if (!embedded) {
    return record.diagnostics.filter((diagnostic) => {
      const offset = offsetAt(record.lines, diagnostic.line, diagnostic.column);
      return !record.blocks.some(({ map }) => offset >= map.sourceStart && offset <= map.sourceEnd);
    });
  }
  const { map } = embedded.block;
  const toGenerated = (line: number, column: number) => {
    const offset = map.generatedOffset(offsetAt(record.lines, line, column));
    return offset === undefined ? undefined : locationAt(map.generatedLines, offset);
  };
  return record.diagnostics.flatMap((diagnostic) => {
    const start = toGenerated(diagnostic.line, diagnostic.column);
    if (!start) return [];
    const end = toGenerated(diagnostic.endLine, diagnostic.endColumn) ?? start;
    const mapped = {
      ...diagnostic,
      ...start,
      endLine: end.line,
      endColumn: end.column,
    };
    if (diagnostic.fix) {
      const edits: JsTextEdit[] = [];
      for (const edit of diagnostic.fix.edits) {
        const editStart = toGenerated(edit.rangeStartLine, edit.rangeStartColumn);
        const editEnd = toGenerated(edit.rangeEndLine, edit.rangeEndColumn);
        if (!editStart || !editEnd) break;
        edits.push({
          ...edit,
          rangeStartLine: editStart.line,
          rangeStartColumn: editStart.column,
          rangeEndLine: editEnd.line,
          rangeEndColumn: editEnd.column,
        });
      }
      if (edits.length === diagnostic.fix.edits.length) mapped.fix = { ...diagnostic.fix, edits };
      else delete mapped.fix;
    }
    return [mapped];
  });
}
