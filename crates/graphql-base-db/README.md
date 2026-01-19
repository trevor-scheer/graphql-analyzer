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
- **FileEntry**: Bundles FileContent and FileMetadata for a single file (for granular caching)
- **FileEntryMap**: Maps FileId to FileEntry for per-file granular invalidation
- **SchemaFileIds / DocumentFileIds**: Track file IDs by type (stable across content changes)
- **ProjectFiles**: Top-level input combining all file tracking
- **file_lookup**: Query to look up a single file's content and metadata
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
- [ ] Additional utility methods

## Granular Per-File Caching

The crate implements true per-file granular caching using `FileEntry` and `FileEntryMap`:

### How It Works

1. **FileEntry**: Each file has its own `FileEntry` Salsa input bundling `FileContent` and `FileMetadata`
2. **FileEntryMap**: A `HashMap<FileId, FileEntry>` stored in a Salsa input
3. **file_lookup query**: Looks up a single file's content and metadata

### Key Insight: Content Updates Don't Change the Map

When a file's content changes:
- Only `FileContent.set_text()` is called
- The `FileEntryMap` HashMap reference stays the **same** (same `Arc`)
- The `FileEntry` struct still points to the same `FileContent` (which has updated text)
- Result: Only queries depending on THIS file's content are invalidated

```rust
// In FileRegistry::add_file for existing files:
if let Some(&existing_content) = self.id_to_content.get(&existing_id) {
    existing_content.set_text(db).to(new_content);  // Only this changes!
    // FileEntryMap is NOT updated - same Arc reference
    return (existing_id, existing_content, metadata, false);
}
```

### Why This Matters

Without granular caching, editing file A would invalidate queries for ALL files:
- Old approach: `FileMap` stored all files in one HashMap
- Any content change created a new HashMap Arc
- ALL file queries would re-run

With granular caching:
- Editing file A only invalidates file A's queries
- File B's queries remain fully cached
- Enables O(1) incremental updates instead of O(n)

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

## Query Tracking for Testing

The crate includes a `tracking` module (behind `test-utils` feature) for verifying Salsa caching behavior in tests. This enables deterministic assertions about incremental computation without relying on timing-based benchmarks.

### Design Principles

1. **Per-database tracking**: Each `TrackedDatabase` has its own query log, avoiding global state and parallel test interference
2. **Checkpoint-based assertions**: Tests use `checkpoint()` and `count_since()` for deterministic assertions
3. **Query name constants**: The `queries` module provides constants to prevent typos

### Usage

```rust
use graphql_db::tracking::{TrackedDatabase, queries};

let mut db = TrackedDatabase::new();
// ... setup files ...

// Take a checkpoint before the operation
let checkpoint = db.checkpoint();

// Call queries
let result = schema_types(&db, project_files);

// Assert on executions since checkpoint
assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint), 1);

// Second call should be cached (0 new executions)
let checkpoint2 = db.checkpoint();
let result2 = schema_types(&db, project_files);
assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint2), 0); // Cached!
```

### Available Query Constants

```rust
pub mod queries {
    pub const PARSE: &str = "parse";
    pub const FILE_STRUCTURE: &str = "file_structure";
    pub const FILE_TYPE_DEFS: &str = "file_type_defs";
    pub const FILE_FRAGMENTS: &str = "file_fragments";
    pub const FILE_OPERATIONS: &str = "file_operations";
    pub const SCHEMA_TYPES: &str = "schema_types";
    pub const ALL_FRAGMENTS: &str = "all_fragments";
    pub const ALL_OPERATIONS: &str = "all_operations";
    pub const FILE_LOOKUP: &str = "file_lookup";
}
```

### API

| Method | Description |
|--------|-------------|
| `TrackedDatabase::new()` | Create a new tracked database with event tracking |
| `checkpoint()` | Get current log position for later comparison |
| `count_since(query, checkpoint)` | Count executions of a query since checkpoint |
| `executions_since(checkpoint)` | Get all query names executed since checkpoint (for debugging) |
| `total_count(query)` | Get total execution count since creation |
| `all_counts()` | Get all query counts as a HashMap |
| `reset()` | Reset all tracking data |

### Enabling the Feature

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
graphql-db = { path = "../graphql-db", features = ["test-utils"] }
```

## References

- [Salsa Framework](https://github.com/salsa-rs/salsa)
- [Rust-Analyzer Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html)
- [LSP Rearchitecture Plan](../../.claude/notes/active/lsp-rearchitecture/README.md)
