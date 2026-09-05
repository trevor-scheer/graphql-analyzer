import * as path from "node:path";
import type { JsDiagnostic, JsFix, JsTextEdit } from "./binding";
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
  diagnostics?: Map<string, JsDiagnostic[]>;
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
  lint: (filename: string, source: string, overrides?: Record<string, unknown>) => JsDiagnostic[],
  overrides: Record<string, unknown>,
): JsDiagnostic[] | undefined {
  const embedded = findEmbeddedBlock(filename);
  const record = embedded?.record ?? getEmbeddedRecord(filename);
  if (!record) return undefined;
  const key = stableJson(overrides);
  record.diagnostics ??= new Map();
  let diagnostics = record.diagnostics.get(key);
  if (!diagnostics) {
    diagnostics = lint(record.filename, record.source, overrides);
    record.diagnostics.set(key, diagnostics);
  }
  if (!embedded) {
    return diagnostics.filter((diagnostic) => {
      const offset = offsetAt(record.lines, diagnostic.line, diagnostic.column);
      return !record.blocks.some(({ map }) => offset >= map.sourceStart && offset <= map.sourceEnd);
    });
  }
  const { map } = embedded.block;
  const toGenerated = (line: number, column: number) => {
    const offset = map.generatedOffset(offsetAt(record.lines, line, column));
    return offset === undefined ? undefined : locationAt(map.generatedLines, offset);
  };
  const mapFix = (fix: JsFix): JsFix | undefined => {
    const edits: JsTextEdit[] = [];
    for (const edit of fix.edits) {
      const start = toGenerated(edit.rangeStartLine, edit.rangeStartColumn);
      const end = toGenerated(edit.rangeEndLine, edit.rangeEndColumn);
      if (!start || !end) return undefined;
      edits.push({
        ...edit,
        rangeStartLine: start.line,
        rangeStartColumn: start.column,
        rangeEndLine: end.line,
        rangeEndColumn: end.column,
      });
    }
    return { ...fix, edits };
  };
  return diagnostics.flatMap((diagnostic) => {
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
      const fix = mapFix(diagnostic.fix);
      if (fix) mapped.fix = fix;
      else delete mapped.fix;
    }
    if (diagnostic.suggestions) {
      mapped.suggestions = diagnostic.suggestions.flatMap((suggestion) => {
        const fix = mapFix(suggestion.fix);
        return fix ? [{ ...suggestion, fix }] : [];
      });
    }
    return [mapped];
  });
}

function stableJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableJson((value as Record<string, unknown>)[key])}`)
    .join(",")}}`;
}
