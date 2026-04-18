const binding = require("@graphql-analyzer/napi");

export interface JsDiagnostic {
  rule: string;
  message: string;
  severity: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  fix?: JsFix;
  help?: string;
  url?: string;
  source: string;
}

export interface JsFix {
  description: string;
  edits: JsTextEdit[];
}

export interface JsTextEdit {
  rangeStartLine: number;
  rangeStartColumn: number;
  rangeEndLine: number;
  rangeEndColumn: number;
  newText: string;
}

export interface JsExtractedBlock {
  source: string;
  offset: number;
  tag?: string;
}

export interface JsRuleMeta {
  name: string;
  description: string;
  defaultSeverity: string;
  category: string;
}

const fileCache = new Map<string, JsDiagnostic[]>();

export function lintFile(path: string, source: string): JsDiagnostic[] {
  const key = `${path}\0${source.length}`;
  const cached = fileCache.get(key);
  if (cached) return cached;

  const result: JsDiagnostic[] = binding.lintFile(path, source);
  fileCache.set(key, result);
  return result;
}

export function extractGraphql(source: string, language: string): JsExtractedBlock[] {
  return binding.extractGraphql(source, language);
}

export function getRules(): JsRuleMeta[] {
  return binding.getRules();
}

export function init(configPath: string): void {
  binding.init(configPath);
}

export function clearCache(): void {
  fileCache.clear();
}
