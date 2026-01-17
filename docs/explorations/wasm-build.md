# WASM Build Exploration

**Issue**: #418
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores compiling the GraphQL language service to WebAssembly for browser-based use cases.

## Goals

1. Enable browser-based GraphQL validation without a server
2. Support online IDEs (CodeSandbox, StackBlitz)
3. Power interactive documentation sites
4. Provide a foundation for web-based schema designers

## Technical Analysis

### Rust to WASM Compilation

The codebase is well-suited for WASM compilation:

**Favorable factors:**
- Pure Rust with no native dependencies
- Salsa is synchronous (no async runtime needed)
- No file system access required (can pass sources as strings)
- No network access required in core (introspection is optional)

**Challenges:**
- Bundle size optimization needed
- `apollo-compiler` and `apollo-parser` add significant code
- Salsa's incremental computation may have overhead in WASM

### Proposed Crate Structure

```
crates/
├── graphql-wasm/           # WASM bindings
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs          # wasm-bindgen exports
```

### Dependencies

```toml
[dependencies]
wasm-bindgen = "0.2"
serde = { version = "1", features = ["derive"] }
serde-wasm-bindgen = "0.6"
graphql-ide = { path = "../graphql-ide" }

[lib]
crate-type = ["cdylib"]
```

### API Design

#### Simple API (Stateless)

```typescript
// Single-shot validation
export function validate(schema: string, document: string): Diagnostic[];
export function lint(schema: string, document: string, config?: LintConfig): Diagnostic[];
```

#### Advanced API (Stateful)

```typescript
// Persistent analysis with incremental updates
export class Analysis {
  constructor();
  setSchema(path: string, content: string): void;
  setDocument(path: string, content: string): void;
  removeFile(path: string): void;

  diagnostics(path: string): Diagnostic[];
  hover(path: string, position: Position): HoverInfo | null;
  gotoDefinition(path: string, position: Position): Location | null;
}
```

### Type Definitions

```typescript
interface Diagnostic {
  severity: 'error' | 'warning' | 'info';
  message: string;
  range: Range;
  source?: string;
}

interface Range {
  start: Position;
  end: Position;
}

interface Position {
  line: number;
  character: number;
}
```

## Bundle Size Analysis

Estimated sizes (uncompressed):

| Component | Size |
|-----------|------|
| apollo-parser | ~200KB |
| apollo-compiler | ~300KB |
| Salsa runtime | ~50KB |
| graphql-* crates | ~150KB |
| wasm-bindgen glue | ~20KB |
| **Total** | **~720KB** |

With wasm-opt and gzip: **~180KB**

### Size Optimization Strategies

1. **Feature flags** - exclude unused components
2. **wasm-opt** - optimize WASM binary
3. **Tree shaking** - remove unused code paths
4. **Lazy loading** - split schema/document validation

## Build Configuration

```toml
# .cargo/config.toml
[target.wasm32-unknown-unknown]
rustflags = ["-C", "opt-level=z"]  # Optimize for size

# Cargo.toml
[profile.release]
lto = true
opt-level = "z"
strip = true
```

### Build Script

```bash
#!/bin/bash
wasm-pack build crates/graphql-wasm --target web --release
wasm-opt -Oz pkg/graphql_wasm_bg.wasm -o pkg/graphql_wasm_bg.wasm
```

## NPM Package Structure

```
@graphql-lsp/wasm/
├── package.json
├── graphql_wasm.js       # JS glue
├── graphql_wasm.d.ts     # TypeScript types
├── graphql_wasm_bg.wasm  # WASM binary
└── README.md
```

## Use Case: Online Playground

```html
<script type="module">
import init, { validate } from '@graphql-lsp/wasm';

await init();

const schema = `type Query { hello: String }`;
const query = `query { hello }`;

const diagnostics = validate(schema, query);
if (diagnostics.length === 0) {
  console.log('Valid!');
} else {
  diagnostics.forEach(d => console.error(d.message));
}
</script>
```

## Use Case: Monaco Editor Integration

```typescript
import init, { Analysis } from '@graphql-lsp/wasm';
import * as monaco from 'monaco-editor';

await init();
const analysis = new Analysis();
analysis.setSchema('schema.graphql', schemaContent);

monaco.editor.onDidChangeModelContent((e) => {
  const content = editor.getValue();
  analysis.setDocument('query.graphql', content);

  const diagnostics = analysis.diagnostics('query.graphql');
  monaco.editor.setModelMarkers(model, 'graphql',
    diagnostics.map(d => ({
      severity: monaco.MarkerSeverity.Error,
      message: d.message,
      startLineNumber: d.range.start.line + 1,
      startColumn: d.range.start.character + 1,
      endLineNumber: d.range.end.line + 1,
      endColumn: d.range.end.character + 1,
    }))
  );
});
```

## Open Questions

1. **Async initialization**: Should `init()` be required, or can we auto-initialize?
2. **Memory management**: How to handle large schemas without memory pressure?
3. **Worker support**: Should we provide a Web Worker wrapper for background validation?
4. **Streaming**: Can we support streaming validation for very large documents?

## Next Steps

1. [ ] Verify apollo-rs compiles to WASM target
2. [ ] Create minimal `graphql-wasm` crate
3. [ ] Benchmark cold vs warm validation
4. [ ] Measure actual bundle size
5. [ ] Test in major browsers (Chrome, Firefox, Safari)
6. [ ] Create demo page

## References

- [wasm-bindgen documentation](https://rustwasm.github.io/wasm-bindgen/)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- [Rust and WebAssembly book](https://rustwasm.github.io/docs/book/)
