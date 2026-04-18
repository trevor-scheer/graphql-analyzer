import * as path from "path";
import * as fs from "fs";

const binding = require("@graphql-analyzer/napi");

const CONFIG_FILENAMES = [
  ".graphqlrc.yaml",
  ".graphqlrc.yml",
  ".graphqlrc.json",
  ".graphqlrc",
  "graphql.config.yaml",
  "graphql.config.yml",
  "graphql.config.json",
  "graphql.config.js",
  "graphql.config.ts",
];

let initialized = false;

function findConfigFile(startDir: string): string | null {
  let dir = startDir;
  while (true) {
    for (const name of CONFIG_FILENAMES) {
      const candidate = path.join(dir, name);
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function ensureInitialized(filePath: string): void {
  if (initialized) return;
  const dir = path.dirname(filePath);
  const configPath = findConfigFile(dir);
  if (configPath) {
    try {
      binding.init(configPath);
    } catch {
      // Config loading may fail — continue without project context
    }
  }
  initialized = true;
}

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

export function lintFile(filePath: string, source: string): JsDiagnostic[] {
  ensureInitialized(filePath);

  const key = `${filePath}\0${source.length}`;
  const cached = fileCache.get(key);
  if (cached) return cached;

  const result: JsDiagnostic[] = binding.lintFile(filePath, source);
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
