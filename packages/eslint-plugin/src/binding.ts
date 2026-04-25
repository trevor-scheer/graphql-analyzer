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

// One-time init is keyed by resolved config path rather than a global flag.
// ESLint invocations that span multiple projects (monorepos) then get the
// right config per file rather than whichever project's config loaded first.
// Rust-side Salsa also tolerates repeated init calls for the same config.
const initializedConfigs = new Set<string>();
const missingConfigDirs = new Set<string>();

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
  const dir = path.dirname(filePath);
  if (missingConfigDirs.has(dir)) return;

  const configPath = findConfigFile(dir);
  if (!configPath) {
    missingConfigDirs.add(dir);
    return;
  }

  const resolved = path.resolve(configPath);
  if (initializedConfigs.has(resolved)) return;

  try {
    coreBinding.init(resolved);
    initializedConfigs.add(resolved);
  } catch (err) {
    // Surface malformed configs instead of silently linting without project
    // context — otherwise the user just sees "no diagnostics" with no signal.
    const message = err instanceof Error ? err.message : String(err);
    console.warn(
      `[@graphql-analyzer/eslint-plugin] Failed to load config at ${resolved}: ${message}`,
    );
    // Remember we tried; don't retry on every file.
    initializedConfigs.add(resolved);
  }
}

export function lintFile(filePath: string, source: string): JsDiagnostic[] {
  ensureInitialized(filePath);
  // No TS-side cache: the Rust analyzer is Salsa-memoized, and any cache here
  // would need a content hash (not just length) to avoid stale results.
  return coreBinding.lintFile(filePath, source);
}

export function extractGraphql(source: string, language: string): JsExtractedBlock[] {
  return coreBinding.extractGraphql(source, language);
}

export function getRules(): JsRuleMeta[] {
  return coreBinding.getRules();
}
