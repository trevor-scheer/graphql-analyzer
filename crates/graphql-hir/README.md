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

5. **Testing**:
   - Unit tests for structure extraction
   - Unit tests for body extraction
   - Integration tests comparing with current `SchemaIndex`/`DocumentIndex`
   - Incremental recomputation tests

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

Compared to the current `graphql-project` implementation:

| Current | HIR (Phase 2) |
|---------|---------------|
| Manual index updates | Automatic via Salsa |
| Coarse-grained invalidation | Fine-grained structure/body split |
| Eager processing | Lazy evaluation |
| Global locks | Lock-free queries |
| Manual dependency tracking | Automatic via Salsa |

## Integration

This crate will be used by:
- `graphql-analysis` - Validation and linting (Phase 3)
- `graphql-ide` - Language features (Phase 4)
- `graphql-lsp` - LSP protocol adapter (Phase 5)

## References

- [Phase 2 Design Document](../../../.claude/notes/active/lsp-rearchitecture/02-SEMANTICS.md)
- [Rust-Analyzer HIR Layer](https://rust-analyzer.github.io/book/contributing/architecture.html#HIR)
- [Salsa Documentation](https://github.com/salsa-rs/salsa)
