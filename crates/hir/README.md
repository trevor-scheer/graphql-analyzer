# GraphQL HIR (High-level Intermediate Representation)

## Phase 2: Semantics Layer

This crate provides semantic queries on top of syntax, implementing the "golden invariant":

> **"Editing a document's body never invalidates global schema knowledge"**

## Architecture

The HIR layer separates **structure** from **bodies**:

- **Structure** (stable): Type names, field signatures, operation names, fragment names
- **Bodies** (dynamic): Selection sets, field selections, directives

This separation enables fine-grained incremental recomputation via Salsa.

## Current Implementation Status

### ✅ Implemented

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
   - `operation_fragment_deps()` - Direct fragment dependencies (simplified)

### ⚠️ Known Limitations (Phase 2)

1. **Apollo-Parser API Mismatch**: Current implementation uses `apollo_parser::ast` but version 0.8 uses `cst` (Concrete Syntax Tree). Needs refactoring to use CST API.

2. **Salsa Tracked Struct Syntax**: Some tracked structs need lifetime parameter adjustments to compile with Salsa 0.25.

3. **FileRegistry Missing**: The `GraphQLHirDatabase` trait has stub methods for `schema_files()` and `document_files()` that return empty vectors. A proper FileRegistry needs to be implemented to map FileIds to FileContent/FileMetadata.

4. **Transitive Fragment Resolution**: The `operation_fragment_deps()` query currently only returns direct dependencies. Full transitive resolution requires FileRegistry to look up fragment files.

### 📋 TODO (Future Phases)

1. **Fix Apollo-Parser Integration**:
   - Update all `apollo_parser::ast` imports to `apollo_parser::cst`
   - Update all AST node traversal to use CST API
   - Test with actual GraphQL documents

2. **Implement FileRegistry**:
   - Add Salsa-tracked registry mapping FileId → (FileContent, FileMetadata)
   - Update database trait methods to return actual file data
   - Add methods for registering/unregistering files

3. **Complete Fragment Resolution**:
   - Implement full transitive fragment dependency resolution
   - Handle circular fragment references gracefully

4. **Add Type Queries**:
   - `type_fields()` - Get all fields of a type (with extensions merged)
   - `field_data()` - Get detailed field information
   - `type_by_name()` - Look up types by name

## Caching Verification Tests

The crate includes comprehensive tests that verify Salsa's incremental computation is working correctly. These tests use `TrackedHirDatabase` (a local database type with query tracking) to make deterministic assertions about caching behavior.

### Tests Included

| Test                                                         | What It Verifies                                             |
| ------------------------------------------------------------ | ------------------------------------------------------------ |
| `test_cache_hit_on_repeated_query`                           | Repeated queries don't re-execute (served from cache)        |
| `test_granular_caching_editing_one_file`                     | Editing file A doesn't invalidate queries for file B         |
| `test_unrelated_file_edit_doesnt_invalidate_schema`          | Document changes don't affect schema queries                 |
| `test_editing_one_of_many_files_is_o1_not_on`                | O(1) recomputation when editing 1 of N files                 |
| `test_fragment_index_not_invalidated_by_unrelated_edit`      | Fragment cache stable across non-fragment edits              |
| `test_golden_invariant_schema_stable_across_operation_edits` | **Critical**: Schema queries never re-run on operation edits |
| `test_executions_since_for_debugging`                        | Debugging helper works correctly                             |

### Golden Invariant Test

The most important test verifies the architectural invariant:

```rust
// Edit BOTH operation files (simulating active development)
op1_content.set_text(&mut db).to(Arc::from("query GetUsers { users { id name } }"));
op2_content.set_text(&mut db).to(Arc::from("query GetUserNames { users { name email } }"));

// Re-query schema - should be COMPLETELY cached
let types_after = schema_types(&db, project_files);

// GOLDEN INVARIANT: schema_types should NOT re-execute
assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint), 0);
```

This ensures IDE responsiveness: users edit operations frequently, and we must NOT re-compute schema knowledge on every keystroke.

## Design Principles

1. **Query-Based**: All derived data computed via Salsa queries, not imperative updates
2. **Lazy Evaluation**: Body queries only run when needed for validation
3. **Automatic Memoization**: Salsa handles caching and invalidation
4. **Immutable Data**: All types are `Clone` and never modified in place
5. **Fine-Grained Invalidation**: Structure changes don't invalidate unrelated bodies

## Example Usage (Future)

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

This crate will be used by:

- `graphql-analysis` - Validation and linting (Phase 3)
- `graphql-ide` - Language features (Phase 4)
- `graphql-lsp` - LSP protocol adapter (Phase 5)

## References

- [Phase 2 Design Document](../../../.claude/notes/active/lsp-rearchitecture/02-SEMANTICS.md)
- [Rust-Analyzer HIR Layer](https://rust-analyzer.github.io/book/contributing/architecture.html#HIR)
- [Salsa Documentation](https://github.com/salsa-rs/salsa)
