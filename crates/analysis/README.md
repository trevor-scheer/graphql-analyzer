# graphql-analysis

Validation and linting layer for GraphQL, built on top of the HIR (High-level Intermediate Representation) layer.

## Overview

This crate provides query-based validation and linting for GraphQL schemas and documents. All validation is implemented as Salsa queries for automatic incrementality and memoization.

## Core Principle: Validation as Queries

Instead of imperative "validate all files" functions, validation is expressed as a set of **queries** that compute diagnostics on demand:

```rust
// Main entry point - get all diagnostics for a file
let diagnostics = file_diagnostics(db, content, metadata);
```

## Architecture

```
file_diagnostics()
├── Syntax errors (from parse)
├── Validation diagnostics (apollo-compiler)
│   ├── Field selection validation against schema
│   ├── Argument validation (required args, correct types)
│   ├── Fragment spread resolution and type checking
│   ├── Variable usage and type validation
│   └── Circular fragment detection
└── Lint diagnostics (from graphql-linter integration)
    ├── Document lints (standalone)
    ├── Document+schema lints
    └── Schema lints
```

### Validation Layers

1. **Syntax Validation** (`graphql-syntax` crate)
   - Parse errors from apollo-parser
   - File-local, cached by Salsa

2. **Apollo-Compiler Validation** (`validation.rs`)
   - Field selection validation against schema types
   - Argument validation (required args, correct types)
   - Fragment spread resolution and type checking
   - Variable usage and type validation
   - Circular fragment detection
   - Type coercion validation
   - Cross-file fragment resolution via transitive dependency collection

3. **Document Validation** (`document_validation.rs`)
   - Operation name uniqueness (project-wide)
   - Fragment name uniqueness (project-wide)
   - Fragment type condition validation
   - Variable type validation
   - Root type checking (Query/Mutation/Subscription)

4. **Lint Integration** (`lint_integration.rs`)
   - Integration with `graphql-linter` crate
   - Document-level lints (standalone, no schema required)
   - Document+schema lints (require schema context)
   - Schema-level lints

5. **Project-Wide Lints** (`project_lints.rs`)
   - Unused fragments (with transitive resolution)
   - Unused fields
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

- `schema_types()` - schema unchanged
- `all_fragments()` - no fragment changes
- Bodies of other operations

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

- `file_structure()` for document files
- `all_fragments()`

**Result:** Schema and all documents revalidated (~10-50ms)

## Diagnostic Types

All diagnostics use a common `Diagnostic` type:

```rust
pub struct Diagnostic {
    pub severity: Severity,  // Error, Warning, Info
    pub message: Arc<str>,
    pub range: DiagnosticRange,
    pub source: Arc<str>,  // "syntax", "validation", "graphql-linter", etc.
    pub code: Option<Arc<str>>,
}
```

Ranges use LSP-style positions (0-indexed line/column).

## Testing

Run tests with:

```bash
cargo test --package graphql-analysis
```

## Dependencies

- `graphql-db` - Salsa database and input types
- `graphql-syntax` - Parsing layer
- `graphql-hir` - Semantic queries
- `graphql-linter` - Linting rules
- `salsa` - Incremental computation framework
