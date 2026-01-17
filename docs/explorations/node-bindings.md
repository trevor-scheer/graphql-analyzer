# Node.js Bindings Exploration

**Issue**: #419
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores creating native Node.js bindings using napi-rs to provide programmatic access to the GraphQL language service from JavaScript/TypeScript.

## Goals

1. Provide native-speed GraphQL validation in Node.js
2. Enable programmatic access for custom tooling
3. Serve as foundation for build tool plugins (Vite, Webpack, esbuild)
4. Support testing utilities (Jest/Vitest integration)

## Technical Analysis

### Why napi-rs?

[napi-rs](https://napi.rs/) is the modern choice for Rust→Node bindings:

- **Type safe**: Generates TypeScript declarations automatically
- **Cross-platform**: Supports macOS, Linux, Windows (x64, arm64)
- **Well maintained**: Active development, used by SWC, Parcel, etc.
- **Performance**: Direct N-API calls, no serialization overhead

### Crate Structure

```
crates/
├── graphql-napi/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── analysis.rs    # Stateful API
│   │   └── simple.rs      # Simple functions
│   └── npm/
│       └── package.json   # npm package template
```

### Dependencies

```toml
[package]
name = "graphql-napi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
napi = { version = "2", features = ["napi9", "serde-json"] }
napi-derive = "2"
serde = { version = "1", features = ["derive"] }
graphql-ide = { path = "../graphql-ide" }
graphql-config = { path = "../graphql-config" }

[build-dependencies]
napi-build = "2"
```

## API Design

### Simple API

```typescript
// @graphql-lsp/core

/**
 * Validate a GraphQL document against a schema
 */
export function validate(
  schema: string,
  document: string,
  options?: ValidateOptions
): Diagnostic[];

/**
 * Lint a GraphQL document
 */
export function lint(
  schema: string,
  document: string,
  options?: LintOptions
): Diagnostic[];

/**
 * Parse a GraphQL document and return syntax errors
 */
export function parse(source: string): ParseResult;

interface ValidateOptions {
  /** File path for error reporting */
  filePath?: string;
}

interface LintOptions {
  /** Lint preset to use */
  preset?: 'recommended' | 'strict' | 'none';
  /** Individual rule configuration */
  rules?: Record<string, 'error' | 'warn' | 'off'>;
}
```

### Stateful API

```typescript
/**
 * Create an analysis instance for incremental validation
 */
export class Analysis {
  constructor(options?: AnalysisOptions);

  // File management
  setSchema(path: string, content: string): void;
  setDocument(path: string, content: string): void;
  removeFile(path: string): void;

  // Diagnostics
  diagnostics(path: string): Diagnostic[];
  allDiagnostics(): Map<string, Diagnostic[]>;

  // IDE features
  hover(path: string, position: Position): HoverInfo | null;
  gotoDefinition(path: string, position: Position): Location | null;
  findReferences(path: string, position: Position): Location[];

  // Schema info
  getSchema(): SchemaInfo | null;
}

interface AnalysisOptions {
  /** Root directory for resolving relative paths */
  rootDir?: string;
  /** Lint configuration */
  lint?: LintOptions;
}
```

### Type Definitions

```typescript
interface Diagnostic {
  severity: 'error' | 'warning' | 'info' | 'hint';
  message: string;
  range: Range;
  source?: string;
  code?: string | number;
  relatedInformation?: DiagnosticRelatedInformation[];
}

interface Range {
  start: Position;
  end: Position;
}

interface Position {
  line: number;      // 0-indexed
  character: number; // 0-indexed (UTF-16 code units)
}

interface Location {
  path: string;
  range: Range;
}

interface HoverInfo {
  contents: string;  // Markdown
  range?: Range;
}

interface SchemaInfo {
  types: TypeInfo[];
  directives: DirectiveInfo[];
}
```

## Implementation

### Simple Functions

```rust
// src/simple.rs
use napi_derive::napi;
use napi::Result;

#[napi]
pub fn validate(schema: String, document: String) -> Result<Vec<Diagnostic>> {
    let host = AnalysisHost::new();
    host.set_schema("schema.graphql", &schema);
    host.set_document("document.graphql", &document);

    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics(&FilePath::new("document.graphql"));

    Ok(diagnostics.into_iter().map(Into::into).collect())
}
```

### Stateful Analysis

```rust
// src/analysis.rs
use napi_derive::napi;
use napi::Result;
use graphql_ide::AnalysisHost;

#[napi]
pub struct Analysis {
    host: AnalysisHost,
}

#[napi]
impl Analysis {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            host: AnalysisHost::new(),
        }
    }

    #[napi]
    pub fn set_schema(&mut self, path: String, content: String) {
        self.host.set_schema(&path, &content);
    }

    #[napi]
    pub fn set_document(&mut self, path: String, content: String) {
        self.host.set_document(&path, &content);
    }

    #[napi]
    pub fn diagnostics(&self, path: String) -> Vec<Diagnostic> {
        let analysis = self.host.analysis();
        analysis.diagnostics(&FilePath::new(&path))
            .into_iter()
            .map(Into::into)
            .collect()
    }
}
```

## Build & Distribution

### Cross-Platform Builds

napi-rs supports building for multiple platforms via GitHub Actions:

```yaml
# .github/workflows/napi.yml
strategy:
  matrix:
    include:
      - os: macos-latest
        target: x86_64-apple-darwin
      - os: macos-latest
        target: aarch64-apple-darwin
      - os: ubuntu-latest
        target: x86_64-unknown-linux-gnu
      - os: ubuntu-latest
        target: aarch64-unknown-linux-gnu
      - os: windows-latest
        target: x86_64-pc-windows-msvc
```

### NPM Package Structure

```
@graphql-lsp/core/
├── package.json
├── index.js           # Platform detection & loading
├── index.d.ts         # TypeScript declarations
├── npm/
│   ├── darwin-x64/
│   │   └── graphql-napi.darwin-x64.node
│   ├── darwin-arm64/
│   │   └── graphql-napi.darwin-arm64.node
│   ├── linux-x64-gnu/
│   │   └── graphql-napi.linux-x64-gnu.node
│   └── win32-x64-msvc/
│       └── graphql-napi.win32-x64-msvc.node
└── README.md
```

### Optional Dependencies Pattern

```json
{
  "name": "@graphql-lsp/core",
  "main": "index.js",
  "types": "index.d.ts",
  "optionalDependencies": {
    "@graphql-lsp/core-darwin-x64": "0.1.0",
    "@graphql-lsp/core-darwin-arm64": "0.1.0",
    "@graphql-lsp/core-linux-x64-gnu": "0.1.0",
    "@graphql-lsp/core-win32-x64-msvc": "0.1.0"
  }
}
```

## Performance Comparison

Expected performance vs graphql-js:

| Operation | graphql-js | @graphql-lsp/core | Speedup |
|-----------|------------|-------------------|---------|
| Parse | 5ms | 0.5ms | 10x |
| Validate | 15ms | 2ms | 7x |
| Full analysis | 25ms | 3ms | 8x |

*Note: Actual benchmarks needed*

## Use Cases

### Custom Validation Script

```typescript
import { validate } from '@graphql-lsp/core';
import { readFileSync } from 'fs';
import { glob } from 'glob';

const schema = readFileSync('schema.graphql', 'utf8');
const files = glob.sync('src/**/*.graphql');

let hasErrors = false;
for (const file of files) {
  const document = readFileSync(file, 'utf8');
  const diagnostics = validate(schema, document);

  for (const d of diagnostics) {
    console.error(`${file}:${d.range.start.line + 1}: ${d.message}`);
    if (d.severity === 'error') hasErrors = true;
  }
}

process.exit(hasErrors ? 1 : 0);
```

### Build Plugin Foundation

```typescript
// Used by @graphql-lsp/vite, @graphql-lsp/webpack, etc.
import { Analysis } from '@graphql-lsp/core';

export function createValidator(schemaPath: string) {
  const analysis = new Analysis();
  const schema = readFileSync(schemaPath, 'utf8');
  analysis.setSchema(schemaPath, schema);

  return {
    validate(path: string, content: string) {
      analysis.setDocument(path, content);
      return analysis.diagnostics(path);
    }
  };
}
```

## Open Questions

1. **Sync vs Async**: Should the API be synchronous or async?
   - Sync is simpler and matches Salsa's model
   - Async might be needed for large schemas

2. **Error handling**: Return errors in result or throw?
   - Validation errors → return in result
   - Programming errors → throw

3. **Config loading**: Should we support loading `.graphqlrc.yaml`?
   - Pro: Familiar configuration
   - Con: Adds complexity, file system access

4. **Thread safety**: Should `Analysis` be shareable across worker threads?

## Next Steps

1. [ ] Set up napi-rs project structure
2. [ ] Implement simple `validate` function
3. [ ] Add TypeScript type generation
4. [ ] Set up cross-platform CI builds
5. [ ] Benchmark against graphql-js
6. [ ] Implement stateful `Analysis` class
7. [ ] Publish to npm (scoped package)

## References

- [napi-rs documentation](https://napi.rs/)
- [SWC's napi implementation](https://github.com/swc-project/swc/tree/main/packages/core)
- [Parcel's napi implementation](https://github.com/parcel-bundler/parcel)
