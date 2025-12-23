# graphql-db

**Status: Work in Progress (Phase 1 - Foundation)**

This crate provides the salsa-based database layer for the GraphQL LSP rearchitecture. It is the foundation for incremental, query-based computation.

## Overview

The `graphql-db` crate defines:

- **FileId**: Unique identifier for files in the project
- **FileUri**: URI/path for files
- **FileKind**: Discriminator for different file types (Schema, ExecutableGraphQL, TypeScript, JavaScript)
- **FileContent**: Salsa input for file content
- **FileMetadata**: Salsa input for file metadata
- **RootDatabase**: The main salsa database

## Architecture

This crate is part of the larger LSP rearchitecture following rust-analyzer patterns:

```
graphql-lsp     ← LSP protocol adapter
    ↓
graphql-ide     ← IDE API with POD types
    ↓
graphql-analysis ← Validation and linting
    ↓
graphql-hir     ← High-level IR (semantic queries)
    ↓
graphql-syntax  ← Parsing (file-local, cached)
    ↓
graphql-db      ← Salsa database (YOU ARE HERE)
```

## Current Status

### Completed ✅
- [x] Basic crate structure created
- [x] Core types defined (FileId, FileUri, FileKind)
- [x] Salsa input structs defined (FileContent, FileMetadata)
- [x] RootDatabase struct defined and implemented
- [x] Salsa 0.25 integration working
- [x] Input query implementations
- [x] Comprehensive tests for database operations
- [x] All tests passing

### In Progress
- [ ] FileRegistry for URI → FileId mapping
- [ ] Complete Change application logic
- [ ] Additional utility methods

### Lessons Learned

**Salsa 0.25 API Pattern**: Successfully integrated salsa 0.25 by reading the official examples. Key insights:

1. The `#[salsa::db]` macro goes on BOTH the struct and the `impl salsa::Database` block
2. The struct must derive `Clone` and `Default`
3. The `storage: salsa::Storage<Self>` field is required
4. The `Setter` trait must be imported to use `.to()` method on field setters
5. Arc types need explicit type annotations in some contexts

**Correct Pattern**:
```rust
#[salsa::db]
#[derive(Clone, Default)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}
```

## Design Goals

Following the rearchitecture plan, this database layer should:

1. **Automatic memoization**: Query results cached by inputs via salsa
2. **Dependency tracking**: Salsa knows what queries depend on what inputs
3. **Incremental invalidation**: When input changes, only affected queries re-run
4. **Cancellation**: Abort stale computations when inputs change
5. **Lazy evaluation**: Queries only run when their results are needed

## Example Usage (Intended)

```rust
// Create database
let mut db = RootDatabase::new();

// Add a file
let content = Arc::from("type Query { hello: String }");
let file_content = FileContent::new(&mut db, content);
let metadata = FileMetadata::new(
    &mut db,
    FileId::new(0),
    FileUri::new("file:///test.graphql"),
    FileKind::Schema,
);

// Update file content (will invalidate dependent queries)
file_content.set_text(&mut db).to(Arc::from("type Query { world: String }"));
```

## References

- [Salsa Framework](https://github.com/salsa-rs/salsa)
- [Rust-Analyzer Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html)
- [LSP Rearchitecture Plan](../../.claude/notes/active/lsp-rearchitecture/README.md)
