# graphql-ide

Editor-facing IDE features for GraphQL language support.

This crate provides the API boundary between the analysis layer (validation, linting) and the LSP layer (protocol handling). It follows rust-analyzer's design philosophy of using Plain Old Data (POD) types with public fields.

## Architecture

```text
LSP Layer (tower-lsp)
    ↓
graphql-ide (this crate) ← POD types, editor API
    ↓
graphql-analysis ← Query-based validation and linting
    ↓
graphql-hir ← Semantic queries
    ↓
graphql-syntax ← Parsing
    ↓
graphql-db ← Salsa database
```

## Core Principles

### 1. POD Types with Public Fields

All types are Plain Old Data structs with public fields:

```rust
pub struct Position {
    pub line: u32,
    pub character: u32,
}

pub struct Range {
    pub start: Position,
    pub end: Position,
}

pub struct Location {
    pub file: FilePath,
    pub range: Range,
}
```

Benefits:
- Easy to construct and destructure
- Easy to serialize/deserialize
- No hidden state or invariants
- Clear ownership semantics

### 2. Editor Coordinates

All positions use editor coordinates (line/column), not byte offsets:
- Lines are 0-indexed
- Characters are 0-indexed (UTF-16 code units for LSP compatibility)
- Ranges are half-open: `[start, end)`

### 3. LSP-Agnostic

The IDE layer knows nothing about the LSP protocol:
- No `tower-lsp` types
- No `lsp-types` dependencies
- Pure Rust data structures
- Easy to test without LSP infrastructure

### 4. Snapshot-Based Concurrency

The `Analysis` type is a cheap, immutable snapshot:
- Implements `Clone` (via salsa's database clone)
- Thread-safe - can be sent to worker threads
- Lock-free reads - all queries through salsa
- Automatically invalidates on changes

## Usage

### Basic Setup

```rust
use graphql_ide::{AnalysisHost, FilePath, FileKind};

// Create the analysis host (owns the database)
let mut host = AnalysisHost::new();

// Add files to the project
host.add_file(
    FilePath::new("schema.graphql"),
    "type Query { hello: String }".to_string(),
    FileKind::Schema,
);

host.add_file(
    FilePath::new("query.graphql"),
    "query { hello }".to_string(),
    FileKind::ExecutableGraphQL,
);

// Get an immutable snapshot for analysis
let analysis = host.snapshot();
```

### Diagnostics

```rust
use graphql_ide::Position;

let file = FilePath::new("schema.graphql");
let diagnostics = analysis.diagnostics(&file);

for diag in diagnostics {
    println!("{:?}: {}", diag.severity, diag.message);
    println!("  at {}:{}", diag.range.start.line, diag.range.start.character);
}
```

### Completions

```rust
let position = Position::new(0, 10);
if let Some(completions) = analysis.completions(&file, position) {
    for item in completions {
        println!("{}: {:?}", item.label, item.kind);
        if let Some(detail) = &item.detail {
            println!("  {}", detail);
        }
    }
}
```

### Hover

```rust
if let Some(hover) = analysis.hover(&file, position) {
    println!("{}", hover.contents);  // Markdown
}
```

### Goto Definition

```rust
if let Some(locations) = analysis.goto_definition(&file, position) {
    for loc in locations {
        println!("{}:{}:{}",
            loc.file.as_str(),
            loc.range.start.line,
            loc.range.start.character
        );
    }
}
```

### Find References

```rust
if let Some(refs) = analysis.find_references(&file, position, true) {
    println!("Found {} references", refs.len());
    for loc in refs {
        // ... print reference locations
    }
}
```

## Concurrency Model

The IDE layer supports two patterns:

### Pattern 1: Single-Threaded

```rust
let mut host = AnalysisHost::new();

loop {
    // Handle file changes
    host.add_file(...);

    // Get snapshot and query
    let analysis = host.snapshot();
    let diagnostics = analysis.diagnostics(...);

    // Send results to editor
    send_diagnostics(diagnostics);
}
```

### Pattern 2: Multi-Threaded

```rust
let mut host = AnalysisHost::new();

// Spawn worker thread
let analysis = host.snapshot();
let handle = std::thread::spawn(move || {
    analysis.diagnostics(&file)
});

// Continue handling file changes on main thread
host.add_file(...);

// Wait for results
let diagnostics = handle.join().unwrap();
```

Salsa ensures that:
- Queries on snapshots are lock-free
- Results are cached and reused
- Changes invalidate only affected queries

## Implementation Status

### ✅ Completed

- POD types (Position, Range, Location, FilePath)
- Feature types (CompletionItem, HoverResult, Diagnostic)
- AnalysisHost and Analysis API structure
- FileRegistry for path mapping
- Database trait implementations
- Basic tests

### ⏳ In Progress

- Diagnostics implementation
- Completions implementation
- Hover implementation
- Goto definition implementation
- Find references implementation
- Comprehensive tests
- Performance benchmarks

## Performance Targets

After full implementation:

| Feature | Target | Notes |
|---------|--------|-------|
| Diagnostics (first call) | <20ms | Parse + validate |
| Diagnostics (cached) | <1ms | Salsa hit |
| Completions | <10ms | O(1) schema lookup |
| Hover | <5ms | O(1) symbol lookup |
| Goto Definition | <5ms | Direct HIR query |
| Find References | <50ms | Lazy search |

## Testing

Run tests:
```bash
cargo test --package graphql-ide
```

Run with coverage:
```bash
cargo tarpaulin --package graphql-ide
```

## Integration with LSP

The graphql-lsp crate will use this API:

```rust
// In graphql-lsp
use graphql_ide::{AnalysisHost, Analysis, Position, FilePath};
use tower_lsp::lsp_types;

struct Backend {
    host: AnalysisHost,
}

impl LanguageServer for Backend {
    async fn did_change(&self, params: DidChangeParams) {
        // Update host
        self.host.add_file(...);

        // Get snapshot and publish diagnostics
        let analysis = self.host.snapshot();
        let diagnostics = analysis.diagnostics(...);

        // Convert IDE types to LSP types
        let lsp_diagnostics = diagnostics.iter()
            .map(|d| to_lsp_diagnostic(d))
            .collect();

        self.client.publish_diagnostics(uri, lsp_diagnostics, None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<CompletionList> {
        let analysis = self.host.snapshot();
        let file = to_file_path(params.text_document_position.uri);
        let pos = to_position(params.text_document_position.position);

        let items = analysis.completions(&file, pos)
            .map(|items| items.iter().map(to_lsp_completion_item).collect())
            .unwrap_or_default();

        Ok(CompletionList { items, is_incomplete: false })
    }
}
```

## Design Rationale

### Why POD Types?

1. **Simplicity**: No methods, no invariants, just data
2. **Flexibility**: Easy to extend without breaking changes
3. **Performance**: No virtual dispatch, easy to inline
4. **Testing**: Easy to construct test data
5. **Serialization**: Trivial to convert to/from JSON

### Why Snapshots?

1. **Concurrency**: Multiple queries in parallel without locks
2. **Consistency**: All queries see the same state
3. **Cancellation**: Drop snapshot to cancel ongoing work
4. **Simplicity**: No explicit synchronization needed

### Why FileRegistry?

1. **Decoupling**: LSP uses URIs, salsa uses FileIds
2. **Type Safety**: FileId is a newtype, prevents mixing with offsets
3. **Efficiency**: O(1) bidirectional lookups
4. **Future**: Can be replaced with project configuration

## Future Work

### Near Term (Phase 4 completion)

- Implement all IDE features
- Add integration tests
- Performance benchmarking
- Complete documentation

### Long Term (Phase 5+)

- Semantic highlighting
- Code actions (quick fixes)
- Rename refactoring
- Workspace symbols
- Document symbols
- Call hierarchy
- Type hierarchy

## Resources

- [Rust-Analyzer IDE Layer](https://github.com/rust-lang/rust-analyzer/tree/master/crates/ide)
- [Salsa Book](https://salsa-rs.github.io/salsa/)
- [LSP Specification](https://microsoft.github.io/language-server-protocol/)
