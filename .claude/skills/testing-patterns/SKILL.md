---
name: testing-patterns
description: Reference for test organization, TestDatabase patterns, and shared test infrastructure. Use when writing new tests or choosing between unit vs integration tests.
user-invocable: true
---

# Testing Patterns

This skill provides detailed guidance on test organization and infrastructure.

## Running Tests

```bash
cargo test                                       # All tests
cargo test --package graphql-linter              # Specific crate
cargo test --package graphql-linter redundant_fields  # Specific test
cargo test -- --nocapture                        # With output
cargo test --test '*'                            # Integration tests only
```

## Unit vs Integration Tests

| Aspect | Unit Tests | Integration Tests |
|--------|------------|-------------------|
| **Location** | `src/*.rs` inline `#[cfg(test)]` | `crates/*/tests/*.rs` |
| **Scope** | ONE Salsa query or helper | Multiple queries together |
| **Database** | Local minimal TestDatabase | `graphql_test_utils::TestDatabase` |
| **Scenarios** | Single-file, isolated | Multi-file, cross-file |

### When to Use Which

**Unit test** if:
- Testing ONE Salsa query or helper function
- Single-file scenario only
- No caching verification needed

**Integration test** if:
- Testing multiple queries working together
- Multi-file or cross-file behavior
- Verifying Salsa caching/invalidation

## TestDatabase Patterns

### Unit Tests (in trait-defining crates: graphql-hir, graphql-analysis)

Use a LOCAL TestDatabase to avoid orphan rule issues:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    #[salsa::db]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

    #[salsa::db]
    impl GraphQLHirDatabase for TestDatabase {}

    #[test]
    fn test_single_query() {
        let db = TestDatabase::default();
        // Test ONE query...
    }
}
```

### Integration Tests

Use shared TestDatabase from graphql-test-utils:

```rust
use graphql_test_utils::{test_project, TestProjectBuilder, TestDatabase};

// Simple single-file
let (db, project) = test_project(
    "type Query { user: User } type User { id: ID! }",
    "query { user { id } }",
);

// Multi-file with builder
let (db, project) = TestProjectBuilder::new()
    .with_schema("schema.graphql", BASIC_SCHEMA)
    .with_document("fragments.graphql", "fragment UserFields on User { id }")
    .with_document("query.graphql", "query { user { ...UserFields } }")
    .build();
```

### Caching Verification Tests

Use TrackedDatabase to verify Salsa caching:

```rust
use graphql_test_utils::tracking::{TrackedDatabase, queries};

#[test]
fn test_cache_hit() {
    let mut db = TrackedDatabase::new();
    // ... set up files

    // First call - cold
    let checkpoint = db.checkpoint();
    let _ = some_query(&db, args);
    assert!(db.count_since(queries::SOME_QUERY, checkpoint) >= 1);

    // Second call - should be cached
    let checkpoint2 = db.checkpoint();
    let _ = some_query(&db, args);
    assert_eq!(db.count_since(queries::SOME_QUERY, checkpoint2), 0);
}
```

## Crate-Specific Guidance

| Crate | Unit Tests | Integration Tests |
|-------|------------|-------------------|
| graphql-base-db | `RootDatabase` directly | N/A (foundation layer) |
| graphql-syntax | None needed (pure parsing) | N/A |
| graphql-hir | Local TestDatabase | `graphql_test_utils::TestDatabase` |
| graphql-analysis | Local TestDatabase | `graphql_test_utils::TestDatabase` |
| graphql-linter | `RootDatabase` | `graphql_test_utils::TestDatabase` |
| graphql-ide, higher | `graphql_test_utils::TestDatabase` | `graphql_test_utils::TestDatabase` |

## Test Utilities

### graphql_base_db::test_utils

```rust
use graphql_base_db::test_utils::create_project_files;

let project_files = create_project_files(
    &mut db,
    &[(schema_id, schema_content, schema_metadata)],
    &[(doc_id, doc_content, doc_metadata)],
);
```

### Cursor Extraction for IDE Tests

```rust
use graphql_test_utils::extract_cursor;

let (source, pos) = extract_cursor("query { user { *name } }");
// pos points to position of '*', '*' removed from source
```

## Writing Readable Tests

- **Use helper functions** to reduce boilerplate
- **Use fixtures** for common schemas/documents
- **Use snapshots** (`cargo-insta`) for complex output
- **Name tests descriptively** - name should explain what's tested
- **Keep focused** - one logical assertion per test

## Performance Benchmarks

```bash
cargo bench                              # Run all benchmarks
cargo bench parse_cold                   # Specific benchmark
cargo bench -- --save-baseline main      # Save baseline
cargo bench -- --baseline main           # Compare against baseline
```

**Expected results:**
- Warm vs Cold: 100-1000x speedup
- Golden Invariant: < 100 nanoseconds
- Fragment Resolution: ~10x speedup with caching

View reports at `target/criterion/report/index.html`.

## Related Skills

- `/audit-tests` - Self-review tests after writing them
