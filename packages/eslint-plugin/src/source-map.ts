import type { Linter, Rule } from "eslint";

export function lineStarts(text: string): number[] {
  const starts = [0];
  for (let index = 0; index < text.length; index++) {
    if (text[index] === "\r") {
      if (text[index + 1] === "\n") index++;
      starts.push(index + 1);
    } else if (text[index] === "\n") starts.push(index + 1);
  }
  return starts;
}

export function offsetAt(starts: number[], line: number, column: number): number {
  return (starts[line - 1] ?? starts[starts.length - 1]) + Math.max(0, column - 1);
}

export function locationAt(starts: number[], offset: number): { line: number; column: number } {
  let low = 0;
  let high = starts.length;
  while (low + 1 < high) {
    const middle = (low + high) >>> 1;
    if (starts[middle] <= offset) low = middle;
    else high = middle;
  }
  return { line: low + 1, column: offset - starts[low] + 1 };
}

export class EmbeddedSourceMap {
  readonly generatedLines: number[];

  constructor(
    readonly text: string,
    readonly starts: number[],
    readonly ends: number[],
    readonly sourceStart: number,
    readonly sourceEnd: number,
    readonly supportsFixes = true,
  ) {
    this.generatedLines = lineStarts(text);
  }

  originalOffset(offset: number, end = false): number {
    const index = Math.max(0, Math.min(offset, this.text.length));
    if (end && index > 0) return this.ends[index - 1];
    return this.starts[index] ?? this.ends[index - 1] ?? this.sourceStart;
  }

  generatedOffset(offset: number): number | undefined {
    if (offset < this.sourceStart || offset > this.sourceEnd) return undefined;
    const found = this.starts.indexOf(offset);
    if (found !== -1) return found;
    const end = this.ends.indexOf(offset);
    return end === -1 ? undefined : end + 1;
  }

  mapFix(fix: Rule.Fix): Rule.Fix | undefined {
    if (!this.supportsFixes) return undefined;
    const [start, end] = fix.range;
    if (start < 0 || end > this.text.length || end < start) return undefined;
    if (/[`\\]|\$\{/.test(fix.text)) return undefined;
    const boundary =
      this.text.slice(Math.max(0, start - 1), start) + fix.text + this.text.slice(end, end + 1);
    if (/\$\{/.test(boundary)) return undefined;
    if (start === end) {
      if (start > 0 && start < this.text.length && this.ends[start - 1] !== this.starts[start]) {
        return undefined;
      }
    } else {
      for (let index = start; index < end; index++) {
        if (this.ends[index] - this.starts[index] !== 1) return undefined;
        if (index > start && this.starts[index] !== this.ends[index - 1]) return undefined;
      }
    }
    return {
      range: [this.originalOffset(start), this.originalOffset(end, start !== end)],
      text: fix.text,
    };
  }

  mapMessage(message: Linter.LintMessage, physicalLines: number[]): Linter.LintMessage {
    const mapped = {
      ...message,
      ...locationAt(
        physicalLines,
        this.originalOffset(offsetAt(this.generatedLines, message.line, message.column)),
      ),
    };
    if (message.endLine !== undefined && message.endColumn !== undefined) {
      const end = locationAt(
        physicalLines,
        this.originalOffset(
          offsetAt(this.generatedLines, message.endLine, message.endColumn),
          true,
        ),
      );
      mapped.endLine = end.line;
      mapped.endColumn = end.column;
    }
    if (message.fix) {
      const fix = this.mapFix(message.fix);
      if (fix) mapped.fix = fix;
      else delete mapped.fix;
    }
    if (message.suggestions) {
      mapped.suggestions = message.suggestions.flatMap((suggestion) => {
        const fix = this.mapFix(suggestion.fix);
        return fix ? [{ ...suggestion, fix }] : [];
      });
      if (!mapped.suggestions.length) delete mapped.suggestions;
    }
    return mapped;
  }
}

export function mapLiteral(
  source: string,
  start: number,
  end: number,
  text: string,
): EmbeddedSourceMap {
  const raw = source.slice(start, end);
  const starts: number[] = [];
  const ends: number[] = [];
  let transformed = "";
  const interpolation = /\$\{[^}]*\}/gy;
  for (let index = 0; index < raw.length; index++) {
    interpolation.lastIndex = index;
    const match = interpolation.exec(raw);
    if (match) {
      index += match[0].length - 1;
      continue;
    }
    const escapedBacktick = raw[index] === "\\" && raw[index + 1] === "`";
    starts.push(start + index);
    if (escapedBacktick) index++;
    ends.push(start + index + 1);
    transformed += raw[index];
  }
  if (transformed === text) return new EmbeddedSourceMap(text, starts, ends, start, end);

  // Custom pluck hooks can trim a literal; only an unambiguous slice is mappable.
  const offset = transformed.indexOf(text);
  if (offset >= 0 && transformed.indexOf(text, offset + 1) === -1) {
    return new EmbeddedSourceMap(
      text,
      starts.slice(offset, offset + text.length),
      ends.slice(offset, offset + text.length),
      start,
      end,
    );
  }
  throw new Error(
    "Cannot map transformed GraphQL text to its source literal; use skipIndent: true and source-preserving pluck hooks.",
  );
}
