# GraphQL Syntax Layer

## Phase 1/2: Parsing and Syntax Trees

This crate handles parsing GraphQL files and TypeScript/JavaScript files with embedded GraphQL. It provides file-local, cacheable parsing via Salsa queries.

## Key Features

1. **File-Local Parsing**: Each file parses independently, fully parallelizable
2. **Salsa Integration**: All parsing results cached and automatically invalidated
3. **TypeScript/JavaScript Support**: Uses `graphql-extract` to find and parse embedded GraphQL
4. **Line Index**: Efficient byte offset to line/column conversion

## API

### Parse Query

```rust
pub fn parse(
    db: &dyn GraphQLSyntaxDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Parse
```

Parses a file and returns:

- Syntax tree (Arc for cheap cloning)
- Extracted blocks (for TS/JS files)
- Parse errors (syntax errors only, not validation)

### Line Index Query

```rust
pub fn line_index(
    db: &dyn GraphQLSyntaxDatabase,
    content: FileContent,
) -> Arc<LineIndex>
```

Computes line boundaries for position conversions.

## Implementation Status

### ✅ Implemented

- Salsa database trait (`GraphQLSyntaxDatabase`)
- Parse query for pure GraphQL files
- Parse query for TypeScript/JavaScript files (uses `graphql-extract`)
- LineIndex for position conversion
- Comprehensive tests

### ✅ Working

The crate compiles and all tests pass. It successfully:

- Parses GraphQL files using apollo-parser
- Extracts and parses GraphQL from TypeScript/JavaScript
- Caches results via Salsa
- Converts between byte offsets and line/column positions

## Usage Example

```rust
use graphql_syntax::{parse, line_index, GraphQLSyntaxDatabase};
use graphql_db::{FileContent, FileMetadata, FileKind};

// Create file content and metadata
let content = FileContent::new(db, Arc::from("type User { id: ID! }"));
let metadata = FileMetadata::new(db, file_id, uri, FileKind::Schema);

// Parse (cached by Salsa)
let parse = parse(db, content, metadata);

// Access syntax tree
for def in parse.tree.document().definitions() {
    // Process definitions
}

// Get line index for position conversions
let line_idx = line_index(db, content);
let (line, col) = line_idx.line_col(42);
```

## Design Principles

1. **No Cross-File Knowledge**: Parsing is purely file-local
2. **No Semantics**: Syntax trees contain no semantic information
3. **Immutable Results**: All return types are `Clone` and immutable
4. **Salsa Handles Caching**: No manual cache management
5. **Value Semantics**: Cheap to clone, thread-safe

## Integration

This crate is used by:

- `graphql-hir` - Extracts semantic structure from syntax trees
- `graphql-analysis` (future) - Validation and linting
- `graphql-ide` (future) - Syntax highlighting, folding, etc.

## References

- [Phase 1 Design Document](../../../.claude/notes/active/lsp-rearchitecture/01-FOUNDATION.md)
- [Rust-Analyzer Syntax Layer](https://rust-analyzer.github.io/book/contributing/architecture.html#syntax)
