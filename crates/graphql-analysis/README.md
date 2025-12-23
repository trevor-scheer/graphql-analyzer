# graphql-analysis

Validation and linting layer for GraphQL, built on top of the HIR (High-level Intermediate Representation) layer.

## Overview

This crate provides query-based validation and linting for GraphQL schemas and documents. All validation is implemented as Salsa queries for automatic incrementality and memoization.

## Core Principle: Validation as Queries

Instead of imperative "validate all files" functions, validation is expressed as a set of **queries** that compute diagnostics on demand:

```rust
// Main entry point - get all diagnostics for a file
let diagnostics = file_diagnostics(db, content, metadata);

// Project-wide analysis (opt-in, expensive)
let project_diagnostics = project_wide_diagnostics(db);
```

## Architecture

```
file_diagnostics()
├── Syntax errors (from parse)
├── Schema validation (for schema files)
├── Document validation (for operations/fragments)
└── Lint diagnostics (from graphql-linter integration)
```

### Validation Layers

1. **Syntax Validation** (`graphql-syntax` crate)
   - Parse errors from apollo-parser
   - File-local, cached by Salsa

2. **Schema Validation** (`schema_validation.rs`)
   - Duplicate type names within a file
   - Conflicts with types in other files
   - Field type existence
   - Interface implementations
   - Union member validity

3. **Document Validation** (`document_validation.rs`)
   - Operation name uniqueness (project-wide)
   - Fragment name uniqueness (project-wide)
   - Fragment type condition validity
   - Field selections against schema (TODO)
   - Variable type validity (TODO)

4. **Lint Integration** (`lint_integration.rs`)
   - Integration with `graphql-linter` crate
   - Document-level lints (require_id_field, deprecated_field, etc.)
   - Schema-level lints (TODO)

5. **Project-Wide Lints** (`project_lints.rs`)
   - Unused fields (expensive, opt-in)
   - Unused fragments
   - Only run when explicitly requested

## Incrementality in Action

### Scenario: User types in an operation

```graphql
query GetUser {
  user {
-   id
+   id
+   email  // adding a field
  }
}
```

**What gets recomputed:**
- `parse(file)` - file content changed
- `file_structure(file)` - structure unchanged (no name change)
- `operation_body(operation)` - body changed
- `validate_document_file(file)` - validates this operation

**What stays cached:**
- `schema_types()` - schema unchanged ✅
- `all_fragments()` - no fragment changes ✅
- Bodies of other operations ✅

**Result:** Only this operation validated, rest cached (~1-5ms vs 50-500ms)

### Scenario: Schema field added

```graphql
type User {
  id: ID!
  name: String!
+ email: String  // new field
}
```

**What gets recomputed:**
- `parse(schema_file)` - schema changed
- `file_structure(schema_file)` - User type structure changed
- `schema_types()` - depends on schema structures
- `validate_document_file(*)` - schema changed, revalidate all documents

**What stays cached:**
- `file_structure()` for document files ✅
- `all_fragments()` ✅

**Result:** Schema and all documents revalidated (~10-50ms)

## Diagnostic Types

All diagnostics use a common `Diagnostic` type:

```rust
pub struct Diagnostic {
    pub severity: Severity,  // Error, Warning, Info
    pub message: Arc<str>,
    pub range: DiagnosticRange,
    pub source: Arc<str>,  // "graphql-parser", "graphql-linter", etc.
    pub code: Option<Arc<str>>,
}
```

Ranges use LSP-style positions (0-indexed line/column).

## Comparison to Current Implementation

| Current | New (Query-Based) |
|---------|-------------------|
| `validate_all_files()` - imperative | `file_diagnostics()` - declarative query |
| Manual dependency tracking | Automatic via Salsa |
| `ValidationMode::Quick/Smart/Full` | Automatic fine-grained invalidation |
| Project-wide lints run on every save | Opt-in, incremental |
| Locking entire indices | Lock-free query evaluation |
| Hard to test individual steps | Each query independently testable |

## Current Status

**Phase 3 (Analysis) - Complete** ✅

- ✅ Core diagnostic types
- ✅ `file_diagnostics()` query
- ✅ Schema validation (basic structure checks)
- ✅ Document validation (name uniqueness)
- ✅ Lint integration (placeholder)
- ✅ Project-wide lints (placeholder)
- ⚠️ Full schema/document validation (TODO)
- ⚠️ Linter bridge (TODO)

## Future Work

### Phase 4: Complete Validation

1. **Schema Validation**
   - Field type existence checking
   - Interface implementation validation
   - Union member validation
   - Directive validation

2. **Document Validation**
   - Field selection validation against schema
   - Variable type checking
   - Fragment spread validation
   - Argument validation

3. **Linter Bridge**
   - Convert HIR to SchemaIndex/DocumentIndex
   - Call graphql-linter methods
   - Convert diagnostics back to our format

### Phase 5: IDE Integration

Use these queries in the LSP for real-time diagnostics with automatic incrementality.

## Testing

Run tests with:

```bash
cargo test --package graphql-analysis
```

Tests verify:
- Diagnostic creation and formatting
- Query behavior with empty database
- Project-wide diagnostics gating

## Dependencies

- `graphql-db` - Salsa database and input types
- `graphql-syntax` - Parsing layer
- `graphql-hir` - Semantic queries
- `graphql-linter` - Existing linting rules
- `graphql-project` - Bridge types (SchemaIndex, DocumentIndex)
- `salsa` - Incremental computation framework
