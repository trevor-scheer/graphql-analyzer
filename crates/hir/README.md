# GraphQL HIR (High-level Intermediate Representation)

This crate provides semantic queries on top of syntax, implementing the cache invariants that enable efficient incremental computation.

## Architecture

The HIR layer separates **structure** from **bodies**:

| Definition  | Structure (stable)                              | Body (dynamic)                |
| ----------- | ----------------------------------------------- | ----------------------------- |
| Schema type | Type name, field names, field types, arguments  | Directives on fields          |
| Operation   | Operation name, operation type (query/mutation) | Selection set, variables used |
| Fragment    | Fragment name, type condition                   | Selection set                 |

**Structure queries** (`schema_types()`, `all_fragments()`, `all_operations()`) return indexes by name.
**Body queries** (`operation_body()`, `fragment_body()`) return the content of those definitions.

This separation enables fine-grained incremental recomputation via Salsa.

## Implementation

1. **HIR Types**:
   - `TypeId`, `FieldId`, `FragmentId`, `OperationId` - Salsa-based identifiers
   - `TypeDef`, `FieldSignature`, `ArgumentDef` - Schema structure types
   - `OperationStructure`, `FragmentStructure` - Document structure types

2. **Structure Module** (`structure.rs`):
   - `file_structure()` query - Extracts names and signatures from files
   - Comprehensive type definition extraction (objects, interfaces, unions, enums, scalars, input objects)
   - Type extension support
   - Field signature extraction with arguments

3. **Body Module** (`body.rs`):
   - `operation_body()` query - Extracts selection sets and fragment spreads
   - `fragment_body()` query - Extracts fragment bodies
   - Selection extraction (fields, fragment spreads, inline fragments)

4. **Global Queries**:
   - `schema_types()` - Collects all types from schema files
   - `all_fragments()` - Collects all fragments from document files
   - `all_operations()` - Collects all operations from document files
   - `operation_fragment_deps()` - Direct fragment dependencies

## Cache Invariant Tests

The crate includes comprehensive tests that verify Salsa's incremental computation is working correctly. These tests use `TrackedHirDatabase` (a local database type with query tracking) to make deterministic assertions about caching behavior.

### Invariants Tested

| Cache Invariant               | Tests                                                                                                                                        |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| **Structure/Body Separation** | `test_structure_body_separation`, `test_golden_invariant_schema_stable_across_operation_edits`                                               |
| **File Isolation**            | `test_granular_caching_editing_one_file`, `test_unrelated_file_edit_doesnt_invalidate_schema`, `test_editing_one_of_many_files_is_o1_not_on` |
| **Index Stability**           | `test_fragment_index_not_invalidated_by_unrelated_edit`                                                                                      |
| **Basic Memoization**         | `test_cache_hit_on_repeated_query`                                                                                                           |

### Structure/Body Separation Test

The most critical test verifies that editing operation bodies doesn't invalidate schema knowledge:

```rust
// Edit BOTH operation files (simulating active development)
op1_content.set_text(&mut db).to(Arc::from("query GetUsers { users { id name } }"));
op2_content.set_text(&mut db).to(Arc::from("query GetUserNames { users { name email } }"));

// Re-query schema - should be COMPLETELY cached
let types_after = schema_types(&db, project_files);

// Structure/Body Separation: schema_types should NOT re-execute
assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint), 0);
```

This ensures IDE responsiveness: users edit operations frequently, and we must NOT re-compute schema knowledge on every keystroke.

## Design Principles

1. **Query-Based**: All derived data computed via Salsa queries, not imperative updates
2. **Lazy Evaluation**: Body queries only run when needed for validation
3. **Automatic Memoization**: Salsa handles caching and invalidation
4. **Immutable Data**: All types are `Clone` and never modified in place
5. **Fine-Grained Invalidation**: Structure changes don't invalidate unrelated bodies

## Example Usage

```rust
use graphql_hir::*;

// Extract file structure (cached by Salsa)
let structure = file_structure(db, file_id, content, metadata);

// Access structure (stable across body edits)
for type_def in structure.type_defs(db) {
    println!("Type: {}", type_def.name);
}

// Get global schema types (depends on all file structures)
let types = schema_types(db);

// Get operation body (lazy, only when needed)
let body = operation_body(db, operation_id);
```

## Benefits

Compared to direct CST access:

| Direct CST             | HIR                               |
| ---------------------- | --------------------------------- |
| Manual traversal       | Semantic queries                  |
| Coarse-grained caching | Fine-grained structure/body split |
| Eager processing       | Lazy evaluation                   |
| Manual invalidation    | Automatic via Salsa               |
| No dependency tracking | Automatic via Salsa               |

## Integration

This crate is used by:

- `graphql-analysis` - Validation and linting
- `graphql-ide` - Language features (hover, go-to-definition, completions, etc.)
- `graphql-lsp` - LSP protocol adapter

## References

- [Rust-Analyzer HIR Layer](https://rust-analyzer.github.io/book/contributing/architecture.html#HIR)
- [Salsa Documentation](https://github.com/salsa-rs/salsa)
