---
name: audit-tests
description: Audit test organization and patterns. Use PROACTIVELY after writing new tests to self-review, or when reviewing test changes in PRs. Checks unit vs integration test placement, TestDatabase patterns, and caching verification.
user-invocable: true
---

# Audit Tests

This skill audits tests for correctness against the project's testing standards. **Use this proactively after writing any new tests** as a form of self-review.

## When to Use

- **After writing new tests** - Always audit your own tests before committing
- **When reviewing PRs** that add or modify tests
- **When refactoring tests** to ensure they remain correctly categorized
- **When unsure** if a test should be unit or integration

## Test Definitions Reference

### Unit Tests

| Criterion | Requirement |
|-----------|-------------|
| **Location** | `src/*.rs` inline `#[cfg(test)] mod tests { ... }` |
| **Scope** | ONE Salsa query or helper function |
| **Database** | Local minimal `TestDatabase` (~15 lines, implements only required traits) |
| **Access** | Can use `use super::*` for private items |
| **Scenarios** | Single-file only |
| **Caching** | No caching/invalidation verification |

### Integration Tests

| Criterion | Requirement |
|-----------|-------------|
| **Location** | `crates/<crate-name>/tests/*.rs` |
| **Scope** | Multiple queries working together |
| **Database** | `graphql_test_utils::TestDatabase` or `TrackedDatabase` |
| **Access** | Public API only (treat crate as external) |
| **Scenarios** | Multi-file, cross-file behavior |
| **Caching** | Caching/invalidation verification when relevant |

## Audit Checklist

For each test or test file, verify:

### 1. Correct Location

- [ ] **Unit tests** are in `#[cfg(test)] mod tests` within source files
- [ ] **Integration tests** are in `crates/<crate>/tests/*.rs`
- [ ] Tests requiring cross-file behavior are NOT in unit test modules

### 2. Correct TestDatabase Pattern

For **unit tests** in crates that define Salsa traits (graphql-hir, graphql-analysis):
- [ ] Uses a LOCAL `TestDatabase` defined in the test module (~15 lines)
- [ ] Only implements traits actually needed by the crate
- [ ] Does NOT use `graphql_test_utils::TestDatabase`

For **integration tests**:
- [ ] Uses `graphql_test_utils::TestDatabase` for standard tests
- [ ] Uses `graphql_test_utils::TrackedDatabase` for caching verification tests
- [ ] Imports from `graphql_test_utils`, not defining its own database

### 3. Correct Scope

Unit tests should:
- [ ] Test ONE Salsa query or ONE helper function
- [ ] NOT test multiple queries working together
- [ ] NOT verify caching behavior

Integration tests should:
- [ ] Test behavior across multiple files OR
- [ ] Test multiple queries working together OR
- [ ] Verify Salsa caching/invalidation

### 4. Proper Test Utilities Usage

- [ ] Uses `graphql_base_db::test_utils::create_project_files` (NOT reimplementing it)
- [ ] Uses `graphql_test_utils::TestProjectBuilder` for complex multi-file setups
- [ ] Uses `TrackedDatabase` with checkpoints for caching tests

## Common Issues to Flag

### Wrong Location

```rust
// BAD: Integration-style test in unit test module
#[cfg(test)]
mod tests {
    #[test]
    fn test_fragments_resolve_across_files() {  // Multi-file = integration!
        // ...
    }
}

// GOOD: Move to crates/graphql-hir/tests/fragment_tests.rs
```

### Wrong TestDatabase

```rust
// BAD: Using graphql_test_utils in unit test of trait-defining crate
// This causes "multiple versions of crate" errors
#[cfg(test)]
mod tests {
    use graphql_test_utils::TestDatabase;  // WRONG for graphql-hir unit tests
}

// GOOD: Local TestDatabase for unit tests
#[cfg(test)]
mod tests {
    #[derive(Default)]
    #[salsa::db]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }
    // ... implement only required traits
}
```

### Testing Multiple Queries in Unit Test

```rust
// BAD: Tests multiple queries together
#[cfg(test)]
mod tests {
    #[test]
    fn test_validation_with_fragments() {
        let structure = file_structure(&db, ...);  // Query 1
        let fragments = all_fragments(&db, ...);   // Query 2
        let diagnostics = validate(&db, ...);      // Query 3
        // This should be an integration test!
    }
}
```

### Missing Caching Verification

```rust
// If testing Salsa caching behavior, use TrackedDatabase
// BAD: Trying to test caching with regular TestDatabase
#[test]
fn test_cache_hit() {
    let db = TestDatabase::default();
    // Can't verify caching without TrackedDatabase!
}

// GOOD: Use TrackedDatabase for caching tests
#[test]
fn test_cache_hit() {
    let mut db = TrackedDatabase::new();
    let checkpoint = db.checkpoint();
    // ... call query
    assert_eq!(db.count_since(queries::SOME_QUERY, checkpoint), 0);
}
```

## Audit Output Format

When auditing, report findings as:

```
## Test Audit Results

### ✅ Correctly Placed
- `crates/graphql-hir/tests/hir_tests.rs` - Integration tests using TestDatabase
- `crates/graphql-analysis/src/validation.rs` - Unit tests with local TestDatabase

### ⚠️ Issues Found

1. **Wrong location**: `crates/graphql-hir/src/lib.rs::test_cross_file_fragments`
   - Issue: Tests multi-file behavior in unit test module
   - Fix: Move to `crates/graphql-hir/tests/`

2. **Wrong TestDatabase**: `crates/graphql-analysis/src/lib.rs::tests`
   - Issue: Uses `graphql_test_utils::TestDatabase` in trait-defining crate
   - Fix: Use local TestDatabase definition

### Recommendations
- [Any general recommendations based on patterns observed]
```

## Self-Review Reminder

**After writing tests, ask yourself:**

1. Does this test ONE thing (unit) or multiple things together (integration)?
2. Does it need files from multiple sources? → Integration
3. Does it verify caching behavior? → Integration with TrackedDatabase
4. Am I in a crate that defines Salsa traits? → Use local TestDatabase for unit tests
