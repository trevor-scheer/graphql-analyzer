import * as path from "path";
import * as fs from "fs";

// The native addon's generated .d.ts lives at the root of
// `@graphql-analyzer/core` alongside `index.js`; require-as-import gives us a
// typed handle without esModuleInterop indirection.
import coreBinding = require("@graphql-analyzer/core");

// Re-exported types so consumers of `binding.ts` don't have to reach into
// the napi package directly. These come straight from the auto-generated
// bindings — any drift is a compile error here.
export type JsDiagnostic = coreBinding.JsDiagnostic;
export type JsFix = coreBinding.JsFix;
export type JsTextEdit = coreBinding.JsTextEdit;
export type JsExtractedBlock = coreBinding.JsExtractedBlock;
export type JsRuleMeta = coreBinding.JsRuleMeta;

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

let activeConfig: { path: string; source: string } | undefined;

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
  const configPath = findConfigFile(path.dirname(path.resolve(filePath)));
  if (!configPath) {
    if (activeConfig) coreBinding.reset();
    activeConfig = undefined;
    return;
  }

  const resolved = path.resolve(configPath);
  try {
    const source = fs.readFileSync(resolved, "utf8");
    if (activeConfig?.path === resolved && activeConfig.source === source) return;
    coreBinding.init(resolved);
    activeConfig = { path: resolved, source };
  } catch (err) {
    coreBinding.reset();
    activeConfig = undefined;
    const message = err instanceof Error ? err.message : String(err);
    console.warn(
      `[@graphql-analyzer/eslint-plugin] Failed to load config at ${resolved}: ${message}`,
    );
  }
}

export function lintFile(filePath: string, source: string): JsDiagnostic[] {
  ensureInitialized(filePath);
  return coreBinding.lintFile(filePath, source);
}

export function extractGraphql(source: string, language: string): JsExtractedBlock[] {
  return coreBinding.extractGraphql(source, language);
}

export function getRules(): JsRuleMeta[] {
  return coreBinding.getRules();
}
