# ESLint Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a napi-rs backed ESLint plugin that serves as a drop-in replacement for `@graphql-eslint/eslint-plugin`, powered by the existing Rust analyzer.

**Architecture:** A new `crates/napi` Rust crate compiled via napi-rs to a `.node` addon, exposing `lint_file`, `extract_graphql`, and `get_rules`. A TypeScript `packages/eslint-plugin` wraps the addon with ESLint parser, processor, rule shims, and config presets. A `test-workspace/eslint-migration` project demonstrates the migration path.

**Tech Stack:** napi-rs (Rust FFI), TypeScript, ESLint flat config API

---

## File Structure

### New Rust files

| File | Responsibility |
|------|---------------|
| `crates/napi/Cargo.toml` | napi-rs crate config |
| `crates/napi/build.rs` | napi-build setup |
| `crates/napi/src/lib.rs` | FFI exports: `init`, `lint_file`, `extract_graphql`, `get_rules` |
| `crates/napi/src/types.rs` | `JsDiagnostic`, `JsFix`, `JsTextEdit`, `ExtractedBlock`, `RuleMeta` |
| `crates/napi/src/host.rs` | `NapiAnalysisHost` — singleton wrapper around `AnalysisHost` |

### New TypeScript files

| File | Responsibility |
|------|---------------|
| `packages/eslint-plugin/package.json` | npm package config |
| `packages/eslint-plugin/tsconfig.json` | TypeScript config |
| `packages/eslint-plugin/src/index.ts` | Plugin entry: exports parser, processor, rules, configs |
| `packages/eslint-plugin/src/parser.ts` | `parseForESLint` — minimal Program AST |
| `packages/eslint-plugin/src/processor.ts` | GraphQL extraction from JS/TS via napi |
| `packages/eslint-plugin/src/rules.ts` | `makeRule` factory + rule registration |
| `packages/eslint-plugin/src/configs.ts` | Preset configs (flat/schema-recommended, etc.) |
| `packages/eslint-plugin/src/binding.ts` | Typed wrapper around napi binding |

### New test workspace files

| File | Responsibility |
|------|---------------|
| `test-workspace/eslint-migration/README.md` | Migration walkthrough |
| `test-workspace/eslint-migration/package.json` | npm project with both plugins |
| `test-workspace/eslint-migration/schema.graphql` | Sample schema |
| `test-workspace/eslint-migration/src/operations.graphql` | Sample operations |
| `test-workspace/eslint-migration/src/component.tsx` | Embedded GraphQL in TS |
| `test-workspace/eslint-migration/.graphqlrc.yaml` | graphql-config for schema/documents |
| `test-workspace/eslint-migration/eslint.config.before.mjs` | graphql-eslint config |
| `test-workspace/eslint-migration/eslint.config.after.mjs` | graphql-analyzer config |
| `test-workspace/eslint-migration/eslint.config.mjs` | Active config (copy of after) |

### Modified files

| File | Change |
|------|--------|
| `Cargo.toml` | Add `crates/napi` to workspace members |
| `.gitignore` | Add `*.node` and napi build artifacts |

---

## Task 1: napi-rs crate scaffold

**Files:**
- Create: `crates/napi/Cargo.toml`
- Create: `crates/napi/build.rs`
- Create: `crates/napi/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `.gitignore`

- [ ] **Step 1: Create `crates/napi/Cargo.toml`**

```toml
[package]
name = "graphql-analyzer-napi"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
napi = { version = "2", features = ["napi9", "serde-json"] }
napi-derive = "2"
serde_json.workspace = true
graphql-ide = { path = "../ide" }
graphql-linter = { path = "../linter" }
graphql-extract = { path = "../extract" }
graphql-config = { path = "../config" }
parking_lot = "0.12"

[build-dependencies]
napi-build = "2"
```

- [ ] **Step 2: Create `crates/napi/build.rs`**

```rust
extern crate napi_build;

fn main() {
    napi_build::setup();
}
```

- [ ] **Step 3: Create minimal `crates/napi/src/lib.rs`**

```rust
use napi_derive::napi;

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
```

- [ ] **Step 4: Add to workspace and .gitignore**

In `Cargo.toml`, add `"crates/napi"` to workspace members.

In `.gitignore`, add:
```
# napi build artifacts
*.node
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p graphql-analyzer-napi`
Expected: successful compilation

- [ ] **Step 6: Commit**

```bash
git add crates/napi/ Cargo.toml .gitignore
git commit -m "scaffold napi-rs crate"
```

---

## Task 2: napi FFI types

**Files:**
- Create: `crates/napi/src/types.rs`
- Modify: `crates/napi/src/lib.rs`

- [ ] **Step 1: Create `crates/napi/src/types.rs`**

These are the napi-compatible types that cross the FFI boundary. They mirror `graphql_ide::Diagnostic` but use flat line/column fields instead of nested `Range`.

```rust
use napi_derive::napi;

#[napi(object)]
pub struct JsDiagnostic {
    pub rule: String,
    pub message: String,
    pub severity: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub fix: Option<JsFix>,
    pub help: Option<String>,
    pub url: Option<String>,
    pub source: String,
}

#[napi(object)]
pub struct JsFix {
    pub description: String,
    pub edits: Vec<JsTextEdit>,
}

#[napi(object)]
pub struct JsTextEdit {
    pub range_start_line: u32,
    pub range_start_column: u32,
    pub range_end_line: u32,
    pub range_end_column: u32,
    pub new_text: String,
}

#[napi(object)]
pub struct JsExtractedBlock {
    pub source: String,
    pub offset: u32,
    pub tag: Option<String>,
}

#[napi(object)]
pub struct JsRuleMeta {
    pub name: String,
    pub description: String,
    pub default_severity: String,
    pub category: String,
}
```

- [ ] **Step 2: Add conversions from IDE types**

Add `impl From<graphql_ide::Diagnostic> for JsDiagnostic` and related conversions in the same file:

```rust
impl From<graphql_ide::Diagnostic> for JsDiagnostic {
    fn from(d: graphql_ide::Diagnostic) -> Self {
        Self {
            rule: d.code.unwrap_or_default(),
            message: d.message,
            severity: match d.severity {
                graphql_ide::DiagnosticSeverity::Error => "error".to_string(),
                graphql_ide::DiagnosticSeverity::Warning => "warning".to_string(),
                graphql_ide::DiagnosticSeverity::Information => "information".to_string(),
                graphql_ide::DiagnosticSeverity::Hint => "hint".to_string(),
            },
            line: d.range.start.line + 1,
            column: d.range.start.character + 1,
            end_line: d.range.end.line + 1,
            end_column: d.range.end.character + 1,
            fix: d.fix.map(|f| JsFix {
                description: f.label,
                edits: f.edits.into_iter().map(|e| JsTextEdit {
                    range_start_line: e.range.start.line + 1,
                    range_start_column: e.range.start.character + 1,
                    range_end_line: e.range.end.line + 1,
                    range_end_column: e.range.end.character + 1,
                    new_text: e.new_text,
                }).collect(),
            }),
            help: d.help,
            url: d.url,
            source: d.source,
        }
    }
}

impl From<graphql_linter::RuleInfo> for JsRuleMeta {
    fn from(r: graphql_linter::RuleInfo) -> Self {
        Self {
            name: r.name.to_string(),
            description: r.description.to_string(),
            default_severity: match r.default_severity {
                graphql_linter::LintSeverity::Off => "off",
                graphql_linter::LintSeverity::Warn => "warn",
                graphql_linter::LintSeverity::Error => "error",
            }.to_string(),
            category: format!("{}", r.category),
        }
    }
}
```

- [ ] **Step 3: Wire up module in lib.rs**

```rust
mod types;

use napi_derive::napi;

pub use types::*;

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p graphql-analyzer-napi`
Expected: successful compilation

- [ ] **Step 5: Commit**

```bash
git add crates/napi/src/
git commit -m "napi FFI boundary types and conversions"
```

---

## Task 3: napi host singleton and `get_rules`

**Files:**
- Create: `crates/napi/src/host.rs`
- Modify: `crates/napi/src/lib.rs`

- [ ] **Step 1: Create `crates/napi/src/host.rs`**

This is the singleton wrapper around `AnalysisHost`. Modeled after `CliAnalysisHost` but designed for the napi lifecycle.

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;

use graphql_ide::{AnalysisHost, DocumentKind, FilePath, Language};

/// Singleton analysis host for the napi binding.
///
/// ESLint processes files one at a time but we need project-wide context.
/// The host initializes lazily on first use and holds the Salsa database
/// for the process lifetime.
static HOST: OnceLock<Mutex<NapiAnalysisHost>> = OnceLock::new();

pub struct NapiAnalysisHost {
    host: AnalysisHost,
    schema_files: Vec<PathBuf>,
    document_files: Vec<PathBuf>,
    initialized: bool,
}

impl NapiAnalysisHost {
    fn new() -> Self {
        Self {
            host: AnalysisHost::new(),
            schema_files: Vec::new(),
            document_files: Vec::new(),
            initialized: false,
        }
    }
}

pub fn get_host() -> &'static Mutex<NapiAnalysisHost> {
    HOST.get_or_init(|| Mutex::new(NapiAnalysisHost::new()))
}
```

- [ ] **Step 2: Implement `get_rules` in lib.rs**

```rust
mod host;
mod types;

use napi_derive::napi;
use types::{JsRuleMeta, JsDiagnostic};

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn get_rules() -> Vec<JsRuleMeta> {
    graphql_linter::all_rule_info()
        .into_iter()
        .map(JsRuleMeta::from)
        .collect()
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p graphql-analyzer-napi`
Expected: successful compilation

- [ ] **Step 4: Commit**

```bash
git add crates/napi/src/
git commit -m "napi host singleton and get_rules export"
```

---

## Task 4: `init` and `lint_file` napi exports

**Files:**
- Modify: `crates/napi/src/host.rs`
- Modify: `crates/napi/src/lib.rs`

- [ ] **Step 1: Add init logic to host.rs**

Add methods to `NapiAnalysisHost` for initialization from a config path. This mirrors `CliAnalysisHost::from_project_config` but lives on the singleton.

```rust
impl NapiAnalysisHost {
    /// Initialize from a graphql-config file path.
    pub fn init_from_config(&mut self, config_path: &Path) -> Result<(), String> {
        let config = graphql_config::load_config(config_path)
            .map_err(|e| format!("Failed to load graphql config: {e}"))?;

        let project = config.default_project()
            .ok_or_else(|| "No default project in graphql config".to_string())?;

        let base_dir = config_path.parent().unwrap_or(Path::new("."));

        // Reset state
        self.host = AnalysisHost::new();
        self.schema_files.clear();
        self.document_files.clear();

        // Load lint config if present
        if let Some(lint_value) = project.lint() {
            if let Ok(lint_config) = serde_json::from_value::<graphql_linter::LintConfig>(lint_value) {
                if lint_config.validate().is_ok() {
                    self.host.set_lint_config(lint_config);
                }
            }
        }

        // Load schema files
        if let Ok(schema_result) = self.host.load_schemas_from_config(&project, base_dir) {
            self.schema_files.extend(schema_result.loaded_paths);
        }

        // Load document files
        if let Some(ref documents_config) = project.documents {
            self.load_documents(documents_config, base_dir, &project)?;
        } else {
            self.host.rebuild_project_files();
        }

        self.initialized = true;
        Ok(())
    }

    /// Lint a single file, returning diagnostics.
    /// If the file content differs from what's on disk (e.g., unsaved editor buffer),
    /// the provided source text takes precedence.
    pub fn lint_file(&mut self, path: &str, source: &str) -> Vec<graphql_ide::Diagnostic> {
        let file_path = FilePath::from_path(Path::new(path));

        // Update file content in the database
        let ext = Path::new(path).extension().and_then(|e| e.to_str());
        let (language, document_kind) = match ext {
            Some("ts" | "tsx") => (Language::TypeScript, DocumentKind::Executable),
            Some("js" | "jsx") => (Language::JavaScript, DocumentKind::Executable),
            Some("graphql" | "gql") => {
                // Determine kind based on whether this path was loaded as schema or document
                let is_schema = self.schema_files.iter().any(|p| p.to_str() == Some(path));
                if is_schema {
                    (Language::GraphQL, DocumentKind::Schema)
                } else {
                    (Language::GraphQL, DocumentKind::Executable)
                }
            }
            _ => (Language::GraphQL, DocumentKind::Executable),
        };

        self.host.add_file(&file_path, source, language, document_kind);

        let snapshot = self.host.snapshot();
        snapshot.all_diagnostics_for_file(&file_path)
    }
}
```

Note: The `load_documents` helper method follows the same pattern as `CliAnalysisHost::load_document_files` but is simpler since we don't need content validation at the napi layer. Full implementation in the actual code.

- [ ] **Step 2: Add `init` and `lint_file` exports to lib.rs**

```rust
#[napi]
pub fn init(config_path: String) -> napi::Result<()> {
    let path = std::path::Path::new(&config_path);
    let mut host = host::get_host().lock();
    host.init_from_config(path)
        .map_err(|e| napi::Error::from_reason(e))
}

#[napi]
pub fn lint_file(path: String, source: String) -> napi::Result<Vec<JsDiagnostic>> {
    let mut host = host::get_host().lock();
    let diagnostics = host.lint_file(&path, &source);
    Ok(diagnostics.into_iter().map(JsDiagnostic::from).collect())
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p graphql-analyzer-napi`
Expected: successful compilation

- [ ] **Step 4: Commit**

```bash
git add crates/napi/src/
git commit -m "napi init and lint_file exports"
```

---

## Task 5: `extract_graphql` napi export

**Files:**
- Modify: `crates/napi/src/lib.rs`

- [ ] **Step 1: Add `extract_graphql` export**

```rust
#[napi]
pub fn extract_graphql(source: String, language: String) -> napi::Result<Vec<types::JsExtractedBlock>> {
    let lang = match language.as_str() {
        "ts" | "tsx" | "typescript" => graphql_extract::Language::TypeScript,
        "js" | "jsx" | "javascript" => graphql_extract::Language::JavaScript,
        _ => return Err(napi::Error::from_reason(
            format!("Unsupported language: {language}. Use 'ts', 'tsx', 'js', or 'jsx'.")
        )),
    };

    let config = graphql_extract::ExtractConfig::default();
    let blocks = graphql_extract::extract_from_source(&source, lang, &config, "<eslint>")
        .map_err(|e| napi::Error::from_reason(format!("Extraction failed: {e}")))?;

    Ok(blocks.into_iter().map(|b| types::JsExtractedBlock {
        source: b.source,
        offset: b.location.offset as u32,
        tag: b.tag_name,
    }).collect())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p graphql-analyzer-napi`
Expected: successful compilation

- [ ] **Step 3: Commit**

```bash
git add crates/napi/src/lib.rs
git commit -m "napi extract_graphql export"
```

---

## Task 6: Build the napi addon locally and verify

**Files:**
- Create: `packages/napi/package.json`
- Create: `packages/napi/.npmignore`

- [ ] **Step 1: Create `packages/napi/package.json`**

```json
{
  "name": "@graphql-analyzer/napi",
  "version": "0.0.1",
  "private": true,
  "main": "index.js",
  "types": "index.d.ts",
  "napi": {
    "name": "graphql-analyzer",
    "triples": {
      "defaults": true,
      "additional": [
        "aarch64-apple-darwin",
        "aarch64-unknown-linux-gnu"
      ]
    }
  },
  "scripts": {
    "build": "napi build --manifest-path ../../crates/napi/Cargo.toml --platform --release",
    "build:debug": "napi build --manifest-path ../../crates/napi/Cargo.toml --platform"
  },
  "devDependencies": {
    "@napi-rs/cli": "^2.18.0"
  }
}
```

- [ ] **Step 2: Create `.npmignore`**

```
target/
Cargo.toml
src/
```

- [ ] **Step 3: Install dependencies and build**

Run:
```bash
cd packages/napi && npm install && npm run build:debug
```

Expected: produces a `graphql-analyzer.<platform>.node` file and generates `index.js` + `index.d.ts`

- [ ] **Step 4: Verify the addon loads**

Run:
```bash
node -e "const b = require('./'); console.log(b.getVersion()); console.log(b.getRules().length, 'rules')"
```

Expected: prints the version and rule count (27 rules)

- [ ] **Step 5: Commit**

```bash
git add packages/napi/
git commit -m "napi package scaffold and local build"
```

---

## Task 7: ESLint plugin scaffold — parser

**Files:**
- Create: `packages/eslint-plugin/package.json`
- Create: `packages/eslint-plugin/tsconfig.json`
- Create: `packages/eslint-plugin/src/parser.ts`

- [ ] **Step 1: Create `packages/eslint-plugin/package.json`**

```json
{
  "name": "@graphql-analyzer/eslint-plugin",
  "version": "0.0.1",
  "private": true,
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "files": ["dist/"],
  "scripts": {
    "build": "tsc",
    "dev": "tsc --watch"
  },
  "dependencies": {
    "@graphql-analyzer/napi": "workspace:*"
  },
  "peerDependencies": {
    "eslint": ">=9.0.0"
  },
  "devDependencies": {
    "eslint": "^9.0.0",
    "typescript": "^5.0.0",
    "@types/node": "^20.0.0"
  }
}
```

- [ ] **Step 2: Create `packages/eslint-plugin/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "lib": ["ES2022"],
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "esModuleInterop": true,
    "declaration": true,
    "skipLibCheck": true,
    "moduleResolution": "node"
  },
  "include": ["src"],
  "exclude": ["node_modules", "dist"]
}
```

- [ ] **Step 3: Create `packages/eslint-plugin/src/parser.ts`**

```typescript
import type { Linter } from "eslint";

interface ParserOptions {
  filePath?: string;
  [key: string]: unknown;
}

interface ESTreePosition {
  line: number;
  column: number;
}

interface ESTreeProgram {
  type: "Program";
  sourceType: "script";
  body: never[];
  tokens: never[];
  comments: never[];
  loc: { start: ESTreePosition; end: ESTreePosition };
  range: [number, number];
}

function lastLineCol(code: string): ESTreePosition {
  const lines = code.split("\n");
  return {
    line: lines.length,
    column: lines[lines.length - 1].length,
  };
}

export function parseForESLint(
  code: string,
  _options?: ParserOptions,
): Linter.ESLintParseResult {
  return {
    ast: {
      type: "Program",
      sourceType: "script",
      body: [],
      tokens: [],
      comments: [],
      loc: { start: { line: 1, column: 0 }, end: lastLineCol(code) },
      range: [0, code.length],
    } as unknown as Linter.ESLintParseResult["ast"],
  };
}
```

- [ ] **Step 4: Verify it compiles**

Run:
```bash
cd packages/eslint-plugin && npm install && npx tsc --noEmit
```

Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add packages/eslint-plugin/
git commit -m "eslint plugin scaffold with parser"
```

---

## Task 8: ESLint plugin — binding wrapper and rules

**Files:**
- Create: `packages/eslint-plugin/src/binding.ts`
- Create: `packages/eslint-plugin/src/rules.ts`

- [ ] **Step 1: Create `packages/eslint-plugin/src/binding.ts`**

Typed wrapper around the napi binding with per-file result caching:

```typescript
import type {
  JsDiagnostic,
  JsExtractedBlock,
  JsRuleMeta,
} from "@graphql-analyzer/napi";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const binding = require("@graphql-analyzer/napi");

const fileCache = new Map<string, JsDiagnostic[]>();

export function lintFile(path: string, source: string): JsDiagnostic[] {
  const key = `${path}\0${source.length}`;
  const cached = fileCache.get(key);
  if (cached) return cached;

  const result: JsDiagnostic[] = binding.lintFile(path, source);
  fileCache.set(key, result);
  return result;
}

export function extractGraphql(
  source: string,
  language: string,
): JsExtractedBlock[] {
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
```

- [ ] **Step 2: Create `packages/eslint-plugin/src/rules.ts`**

Rule shim factory that creates one ESLint rule per analyzer rule:

```typescript
import type { Rule } from "eslint";
import * as binding from "./binding";

/** Convert camelCase to kebab-case for ESLint rule names */
function toKebabCase(name: string): string {
  return name.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
}

/** Map from kebab-case ESLint name to camelCase analyzer name */
const ruleNameMap = new Map<string, string>();

function makeRule(analyzerRuleName: string, description: string): Rule.RuleModule {
  return {
    meta: {
      type: "problem",
      docs: { description },
      schema: [],
    },
    create(context: Rule.RuleContext) {
      return {
        Program() {
          const diagnostics = binding.lintFile(
            context.filename,
            context.sourceCode.text,
          );
          for (const d of diagnostics) {
            if (d.rule !== analyzerRuleName) continue;
            context.report({
              message: d.message,
              loc: {
                start: { line: d.line, column: d.column - 1 },
                end: { line: d.endLine, column: d.endColumn - 1 },
              },
              // Autofix support — edits use line/column ranges from the analyzer.
              // ESLint's fixer API works with character offsets, so we convert
              // using the source text. This is a known limitation for v1 —
              // autofix will be wired up after the core linting path is proven.
            });
          },
        },
      };
    },
  };
}

/** Build all ESLint rules from analyzer rule metadata */
export function buildRules(): Record<string, Rule.RuleModule> {
  const rules: Record<string, Rule.RuleModule> = {};
  const meta = binding.getRules();

  for (const rule of meta) {
    const kebabName = toKebabCase(rule.name);
    ruleNameMap.set(kebabName, rule.name);
    rules[kebabName] = makeRule(rule.name, rule.description);
  }

  return rules;
}
```

- [ ] **Step 3: Verify compilation**

Run: `cd packages/eslint-plugin && npx tsc --noEmit`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add packages/eslint-plugin/src/
git commit -m "eslint plugin binding wrapper and rule shims"
```

---

## Task 9: ESLint plugin — processor

**Files:**
- Create: `packages/eslint-plugin/src/processor.ts`

- [ ] **Step 1: Create `packages/eslint-plugin/src/processor.ts`**

Extracts GraphQL from JS/TS files using the napi binding:

```typescript
import * as path from "path";
import type { Linter } from "eslint";
import * as binding from "./binding";

const JS_EXTENSIONS = new Set([".js", ".jsx", ".ts", ".tsx", ".svelte", ".vue"]);

interface ExtractedBlockInfo {
  offset: number;
  lineOffset: number;
  columnOffset: number;
}

// Track extraction info per file for postprocessing
const extractionMap = new Map<string, ExtractedBlockInfo[]>();

export const processor: Linter.Processor = {
  preprocess(code: string, filename: string) {
    const ext = path.extname(filename);
    if (!JS_EXTENSIONS.has(ext)) {
      return [code];
    }

    const lang = ext.replace(".", "");
    let blocks;
    try {
      blocks = binding.extractGraphql(code, lang);
    } catch {
      // If extraction fails, skip GraphQL linting for this file
      return [code];
    }

    if (blocks.length === 0) {
      return [code];
    }

    // Calculate line/column offsets for each block
    const blockInfos: ExtractedBlockInfo[] = blocks.map((b) => {
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

  postprocess(
    messages: Linter.LintMessage[][],
    filename: string,
  ): Linter.LintMessage[] {
    const blockInfos = extractionMap.get(filename);
    extractionMap.delete(filename);

    if (!blockInfos) {
      // No extraction happened — just pass through
      return messages.flat();
    }

    // Last group is the original file's messages
    const originalMessages = messages.pop()!;

    // Remap GraphQL block messages to host file locations
    const remapped = messages.flatMap((group, i) => {
      const info = blockInfos[i];
      if (!info) return group;

      return group.map((msg) => ({
        ...msg,
        line: msg.line + info.lineOffset,
        column:
          msg.line === 1 ? msg.column + info.columnOffset : msg.column,
        endLine: msg.endLine
          ? msg.endLine + info.lineOffset
          : undefined,
        endColumn:
          msg.endLine === 1 && msg.endColumn
            ? msg.endColumn + info.columnOffset
            : msg.endColumn,
      }));
    });

    return [...remapped, ...originalMessages];
  },

  supportsAutofix: true,
};
```

- [ ] **Step 2: Verify compilation**

Run: `cd packages/eslint-plugin && npx tsc --noEmit`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add packages/eslint-plugin/src/processor.ts
git commit -m "eslint plugin processor for JS/TS extraction"
```

---

## Task 10: ESLint plugin — config presets and entry point

**Files:**
- Create: `packages/eslint-plugin/src/configs.ts`
- Create: `packages/eslint-plugin/src/index.ts`

- [ ] **Step 1: Create `packages/eslint-plugin/src/configs.ts`**

Config presets matching graphql-eslint's naming. Start with `flat/schema-recommended` and `flat/operations-recommended`:

```typescript
import type { Linter } from "eslint";

/** Schema rules recommended for all schemas */
export const schemaRecommended: Linter.RulesRecord = {
  "@graphql-analyzer/description-style": "warn",
  "@graphql-analyzer/naming-convention": "error",
  "@graphql-analyzer/no-hashtag-description": "warn",
  "@graphql-analyzer/no-typename-prefix": "warn",
  "@graphql-analyzer/no-unreachable-types": "warn",
  "@graphql-analyzer/require-deprecation-reason": "warn",
  "@graphql-analyzer/require-description": "warn",
  "@graphql-analyzer/strict-id-in-types": "warn",
  "@graphql-analyzer/unique-enum-value-names": "warn",
};

/** Operations rules recommended for all projects */
export const operationsRecommended: Linter.RulesRecord = {
  "@graphql-analyzer/no-anonymous-operations": "error",
  "@graphql-analyzer/no-deprecated": "warn",
  "@graphql-analyzer/no-duplicate-fields": "error",
  "@graphql-analyzer/unused-variables": "warn",
  "@graphql-analyzer/unused-fragments": "warn",
};

export const configs: Record<string, { rules: Linter.RulesRecord }> = {
  "flat/schema-recommended": { rules: schemaRecommended },
  "flat/operations-recommended": { rules: operationsRecommended },
};
```

- [ ] **Step 2: Create `packages/eslint-plugin/src/index.ts`**

Plugin entry point exporting parser, processor, rules, and configs:

```typescript
import { parseForESLint } from "./parser";
import { processor } from "./processor";
import { buildRules } from "./rules";
import { configs } from "./configs";

export const parser = { parseForESLint };

export { processor, configs };

export const rules = buildRules();

export default {
  parser,
  processor,
  rules,
  configs,
};
```

- [ ] **Step 3: Build the plugin**

Run: `cd packages/eslint-plugin && npx tsc`
Expected: `dist/` directory created with compiled JS + declarations

- [ ] **Step 4: Commit**

```bash
git add packages/eslint-plugin/src/
git commit -m "eslint plugin config presets and entry point"
```

---

## Task 11: Test workspace — eslint-migration

**Files:**
- Create: `test-workspace/eslint-migration/package.json`
- Create: `test-workspace/eslint-migration/.graphqlrc.yaml`
- Create: `test-workspace/eslint-migration/schema.graphql`
- Create: `test-workspace/eslint-migration/src/operations.graphql`
- Create: `test-workspace/eslint-migration/src/component.tsx`
- Create: `test-workspace/eslint-migration/eslint.config.before.mjs`
- Create: `test-workspace/eslint-migration/eslint.config.after.mjs`
- Create: `test-workspace/eslint-migration/eslint.config.mjs`
- Create: `test-workspace/eslint-migration/README.md`

- [ ] **Step 1: Create `package.json`**

```json
{
  "name": "eslint-migration-demo",
  "private": true,
  "scripts": {
    "lint": "eslint .",
    "lint:before": "ESLINT_CONFIG_FILE=eslint.config.before.mjs eslint .",
    "lint:after": "ESLINT_CONFIG_FILE=eslint.config.after.mjs eslint ."
  },
  "devDependencies": {
    "eslint": "^9.0.0",
    "@graphql-eslint/eslint-plugin": "^4.0.0",
    "@graphql-analyzer/eslint-plugin": "workspace:*"
  }
}
```

- [ ] **Step 2: Create `.graphqlrc.yaml`**

```yaml
schema: "schema.graphql"
documents: "src/**/*.{graphql,tsx}"
```

- [ ] **Step 3: Create `schema.graphql`**

A sample schema that triggers several lint rules:

```graphql
type Query {
  user(id: ID!): User
  users: [User!]!
}

type User {
  id: ID!
  name: String!
  email: String!
  posts: [Post!]!
}

# This should trigger no-hashtag-description
# A blog post
type Post {
  id: ID!
  PostTitle: String!
  content: String!
  author: User!
}

type Mutation {
  deleteUser(id: ID!): Boolean
}
```

- [ ] **Step 4: Create `src/operations.graphql`**

Sample operations that trigger document lint rules:

```graphql
query {
  user(id: "1") {
    name
    email
    name
  }
}

query GetUsers {
  users {
    id
    name
  }
}
```

- [ ] **Step 5: Create `src/component.tsx`**

Sample TSX file with embedded GraphQL:

```tsx
import { gql } from "@apollo/client";

const GET_USER = gql`
  query {
    user(id: "1") {
      name
    }
  }
`;

export function UserComponent() {
  return <div>User</div>;
}
```

- [ ] **Step 6: Create `eslint.config.before.mjs`**

The "before" config using graphql-eslint:

```javascript
import graphqlPlugin from "@graphql-eslint/eslint-plugin";

export default [
  {
    files: ["**/*.graphql"],
    languageOptions: {
      parser: graphqlPlugin.parser,
    },
    plugins: {
      "@graphql-eslint": graphqlPlugin,
    },
    rules: {
      "@graphql-eslint/no-anonymous-operations": "error",
      "@graphql-eslint/no-duplicate-fields": "error",
      "@graphql-eslint/no-hashtag-description": "warn",
      "@graphql-eslint/naming-convention": [
        "error",
        {
          FieldDefinition: "camelCase",
        },
      ],
    },
  },
];
```

- [ ] **Step 7: Create `eslint.config.after.mjs`**

The "after" config using graphql-analyzer — identical structure, different plugin:

```javascript
import graphqlPlugin from "@graphql-analyzer/eslint-plugin";

export default [
  {
    files: ["**/*.graphql"],
    languageOptions: {
      parser: graphqlPlugin.parser,
    },
    plugins: {
      "@graphql-analyzer": graphqlPlugin,
    },
    rules: {
      "@graphql-analyzer/no-anonymous-operations": "error",
      "@graphql-analyzer/no-duplicate-fields": "error",
      "@graphql-analyzer/no-hashtag-description": "warn",
      "@graphql-analyzer/naming-convention": [
        "error",
        {
          FieldDefinition: "camelCase",
        },
      ],
    },
  },
];
```

- [ ] **Step 8: Create `eslint.config.mjs`**

Active config — just re-exports the "after" config:

```javascript
export { default } from "./eslint.config.after.mjs";
```

- [ ] **Step 9: Create `README.md`**

```markdown
# ESLint Migration Demo

This workspace demonstrates migrating from `@graphql-eslint/eslint-plugin`
to `@graphql-analyzer/eslint-plugin`.

## Setup

```bash
npm install
```

## The Migration

The migration is a find-and-replace in your ESLint config:

1. Replace `@graphql-eslint/eslint-plugin` with `@graphql-analyzer/eslint-plugin`
2. Replace `@graphql-eslint` with `@graphql-analyzer` in plugin names and rule prefixes

That's it. Rule names and options are identical.

## Compare

Run linting with the old plugin:

```bash
npm run lint:before
```

Run linting with the new plugin:

```bash
npm run lint:after
```

Both should produce equivalent diagnostics.

## Files

| File | Purpose |
|------|---------|
| `eslint.config.before.mjs` | Original graphql-eslint config |
| `eslint.config.after.mjs` | Migrated graphql-analyzer config |
| `eslint.config.mjs` | Active config (uses the "after" config) |
| `schema.graphql` | Sample schema with intentional lint issues |
| `src/operations.graphql` | Sample operations with lint issues |
| `src/component.tsx` | Embedded GraphQL in TypeScript |
```

- [ ] **Step 10: Commit**

```bash
git add test-workspace/eslint-migration/
git commit -m "eslint migration test workspace"
```

---

## Task 12: Integration test — build and run end-to-end

**Files:**
- No new files, just building and testing

- [ ] **Step 1: Build napi addon**

```bash
cd packages/napi && npm run build:debug
```

Expected: `.node` file produced

- [ ] **Step 2: Build eslint plugin**

```bash
cd packages/eslint-plugin && npm install && npx tsc
```

Expected: `dist/` with compiled JS

- [ ] **Step 3: Install test workspace deps**

```bash
cd test-workspace/eslint-migration && npm install
```

- [ ] **Step 4: Run eslint with the graphql-analyzer plugin**

```bash
cd test-workspace/eslint-migration && npx eslint --config eslint.config.after.mjs .
```

Expected: diagnostics reported for:
- Anonymous operation in `operations.graphql` and `component.tsx`
- Duplicate `name` field in `operations.graphql`
- Hashtag description on `Post` type in `schema.graphql`
- `PostTitle` naming convention violation in `schema.graphql`

- [ ] **Step 5: Fix any issues found during integration**

Debug and fix any compilation, loading, or diagnostic issues.

- [ ] **Step 6: Commit any fixes**

```bash
git add -A
git commit -m "integration fixes from end-to-end testing"
```

---

## Task 13: npm platform package scaffolding

**Files:**
- Create: `packages/napi/npm/darwin-arm64/package.json`
- Create: `packages/napi/npm/darwin-x64/package.json`
- Create: `packages/napi/npm/linux-x64-gnu/package.json`
- Create: `packages/napi/npm/linux-arm64-gnu/package.json`
- Create: `packages/napi/npm/win32-x64-msvc/package.json`

- [ ] **Step 1: Create platform package.json files**

Each one follows the same pattern. Example for `darwin-arm64`:

```json
{
  "name": "@graphql-analyzer/napi-darwin-arm64",
  "version": "0.0.1",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "main": "graphql-analyzer.darwin-arm64.node",
  "files": ["graphql-analyzer.darwin-arm64.node"]
}
```

Repeat for each platform, changing `name`, `os`, `cpu`, `main`, and `files` accordingly.

- [ ] **Step 2: Add optionalDependencies to root napi package.json**

```json
"optionalDependencies": {
  "@graphql-analyzer/napi-darwin-arm64": "0.0.1",
  "@graphql-analyzer/napi-darwin-x64": "0.0.1",
  "@graphql-analyzer/napi-linux-x64-gnu": "0.0.1",
  "@graphql-analyzer/napi-linux-arm64-gnu": "0.0.1",
  "@graphql-analyzer/napi-win32-x64-msvc": "0.0.1"
}
```

- [ ] **Step 3: Commit**

```bash
git add packages/napi/
git commit -m "npm platform package scaffolding"
```

---

## Summary of commits

1. `scaffold napi-rs crate`
2. `napi FFI boundary types and conversions`
3. `napi host singleton and get_rules export`
4. `napi init and lint_file exports`
5. `napi extract_graphql export`
6. `napi package scaffold and local build`
7. `eslint plugin scaffold with parser`
8. `eslint plugin binding wrapper and rule shims`
9. `eslint plugin processor for JS/TS extraction`
10. `eslint plugin config presets and entry point`
11. `eslint migration test workspace`
12. `integration fixes from end-to-end testing`
13. `npm platform package scaffolding`
