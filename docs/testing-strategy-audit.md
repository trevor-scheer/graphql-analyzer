# Testing Strategy Audit & Restructuring Plan

**Date**: January 2026
**Issue**: #434

This document provides a comprehensive audit of all Rust test files in the GraphQL LSP codebase, along with a restructuring plan to create consistent, human-friendly testing patterns.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Per-Crate Analysis](#per-crate-analysis)
3. [Key Findings](#key-findings)
4. [Restructuring Plan](#restructuring-plan)
5. [Implementation Phases](#implementation-phases)
6. [Shared Infrastructure Design](#shared-infrastructure-design)
7. [Pattern Reference](#pattern-reference)

---

## Executive Summary

### Current State

| Metric | Value |
|--------|-------|
| Files with `#[cfg(test)]` modules | 33 |
| Duplicate `create_project_files()` implementations | 8+ |
| Duplicate `TestDatabase` definitions | 6 |
| Snapshot testing usage | None |
| Shared test infrastructure | Minimal |

### Key Problems

1. **Massive duplication**: `create_project_files()` is reimplemented 8+ times with nearly identical code
2. **TestDatabase boilerplate**: Each crate defines its own `TestDatabase` with Salsa trait impls (15-25 lines each)
3. **Inconsistent fixture organization**: Some use constants, others inline strings, no shared fixtures
4. **No snapshot testing**: All assertions are manual, making diagnostic message testing verbose
5. **Inconsistent helper patterns**: Some crates use builders, others use functions, some have neither

### Proposed Solution

1. Create `graphql-test-utils` crate with shared infrastructure
2. Adopt `insta` for selective snapshot testing
3. Standardize on builder pattern for complex setup, simple functions for common cases
4. Create shared fixture module for large/reusable schemas
5. Add per-crate `tests/` directories for integration tests

---

## Per-Crate Analysis

### graphql-db

**Location**: `crates/graphql-db/src/lib.rs:520-608`

**Current State**:
- 6 basic unit tests for data structures
- Uses `RootDatabase` directly (no TestDatabase wrapper needed)
- Simple, focused tests

**Tests**:
```
test_database_creation
test_file_id
test_file_uri
test_file_kind
test_file_content_creation
test_file_metadata_creation
test_file_content_update
```

**Assessment**: ✅ Good
- Tests are simple and appropriate for the crate's scope
- No duplication issues
- Clear naming

**Recommendations**:
- Keep as-is, this is the foundation layer
- Consider exporting a `create_project_files()` helper as public API for downstream tests

---

### graphql-syntax

**Location**: `crates/graphql-syntax/src/lib.rs:387-570`

**Current State**:
- 15+ tests for parsing and line index functionality
- No TestDatabase needed (uses `parse_graphql()` directly)
- Good coverage of edge cases

**Tests**:
```
test_line_index_new
test_line_index_line_col
test_parse_graphql
test_parse_graphql_with_error
test_line_index_empty
test_line_index_single_line
test_content_has_schema_definitions_true
test_content_has_schema_definitions_false
test_content_has_schema_definitions_mixed
test_determine_file_kind_typescript
test_determine_file_kind_javascript
test_determine_file_kind_schema
test_determine_file_kind_executable
test_apollo_parser_error_info
test_documents_iterator_pure_graphql
```

**Assessment**: ✅ Good
- Clean, focused tests
- Good coverage of the public API
- Inline fixtures are appropriate (small schemas/queries)

**Recommendations**:
- Add snapshot tests for parser error messages
- Keep inline fixtures (they're small and contextual)

---

### graphql-hir

**Location**: `crates/graphql-hir/src/lib.rs:742-1000+`

**Current State**:
- Custom `TestDatabase` with Salsa traits (15 lines boilerplate)
- Duplicate `create_project_files()` implementation (25 lines)
- Sophisticated incremental computation tests with `AtomicUsize` counters
- Issue-linked tests documenting architectural decisions

**Tests**:
```
test_schema_types_empty
test_file_structure_basic
test_editing_one_file_does_not_recompute_other_files_structure
test_all_fragments_granular_invalidation
```

**Assessment**: ⚠️ Needs Improvement
- **Good**: Excellent incremental computation verification
- **Good**: Issue-linked documentation in tests
- **Bad**: 15 lines TestDatabase boilerplate
- **Bad**: 25 lines create_project_files() duplication

**Recommendations**:
- Move TestDatabase to shared crate
- Move create_project_files() to shared crate
- Keep the incremental computation tests with counters (domain-specific)

---

### graphql-analysis

**Location**: Multiple files in `crates/graphql-analysis/src/`

**Files with tests**:
- `lib.rs` - 1 test
- `validation.rs` - 4+ tests
- `document_validation.rs` - 5+ tests
- `schema_validation.rs` - tests for schema validation
- `diagnostics.rs` - diagnostic tests
- `merged_schema.rs` - schema merging tests
- `project_lints.rs` - lint integration tests

**Current State**:
- **Three different TestDatabase definitions** across files
- `document_validation.rs` has a special TestDatabase with `project_files: Cell<Option<...>>`
- Each file has its own `create_project_files()` implementation
- Inconsistent patterns between files

**Assessment**: ❌ Needs Major Refactoring
- Most duplication in the codebase
- Inconsistent TestDatabase variants
- Hard to understand which helper to use

**Recommendations**:
- Consolidate to single TestDatabase from shared crate
- Standardize create_project_files() usage
- Consider builder pattern for complex validation scenarios
- Add snapshot tests for diagnostic messages

---

### graphql-ide

**Location**: `crates/graphql-ide/src/lib.rs:3450-3700+`

**Current State**:
- Uses `AnalysisHost` directly (public API) - no TestDatabase needed
- Has unique `extract_cursor()` helper for position testing
- Builder pattern tests for `CompletionItem`, `HoverResult`, `Diagnostic`
- Good Salsa snapshot isolation demonstrations

**Tests**:
```
test_analysis_host_creation
test_position_creation
test_extract_cursor_single_line
test_extract_cursor_multiline
test_extract_cursor_start_of_line
test_extract_cursor_graphql_example
test_range_creation
test_file_path_creation
test_completion_item_builder
test_hover_result_builder
test_diagnostic_builder
test_diagnostics_for_valid_file
test_diagnostics_for_nonexistent_file
test_diagnostics_after_file_update
test_conversion_position
test_conversion_range
test_conversion_severity
```

**Assessment**: ✅ Good (with minor improvements)
- Clean API-level testing via AnalysisHost
- Good builder pattern examples
- `extract_cursor()` is a useful pattern to share

**Recommendations**:
- Export `extract_cursor()` to shared test utils (useful for other IDE tests)
- Keep AnalysisHost usage pattern
- Document snapshot lifetime management pattern

---

### graphql-linter

**Location**: Multiple rule files in `crates/graphql-linter/src/rules/`

**Files with tests**:
- `config.rs` - configuration tests
- `diagnostics.rs` - diagnostic types tests
- `rules/no_anonymous_operations.rs`
- `rules/redundant_fields.rs`
- `rules/require_id_field.rs`
- `rules/unused_variables.rs`

**Current State**:
- Uses `RootDatabase` directly (good)
- Each rule file has its own `create_test_project()` helper (slightly different signatures)
- Large `TEST_SCHEMA` constants in each file
- Good rule-specific test coverage

**Example from require_id_field.rs**:
```rust
fn create_test_project(
    db: &dyn GraphQLHirDatabase,
    schema_source: &str,
    document_source: &str,
    document_kind: FileKind,
) -> (FileId, FileContent, FileMetadata, ProjectFiles)
```

**Assessment**: ⚠️ Needs Improvement
- Good: Uses RootDatabase directly
- Good: Each rule file is self-contained
- Bad: Duplicate `create_test_project()` in every rule file
- Bad: Large TEST_SCHEMA constants duplicated

**Recommendations**:
- Share common TEST_SCHEMA fixtures across rules
- Standardize `create_test_project()` signature
- Consider snapshot testing for lint diagnostic messages

---

### graphql-extract

**Location**: `crates/graphql-extract/src/extractor.rs:490-700+`

**Current State**:
- No database needed (pure extraction logic)
- Nested test module: `mod typescript_tests { ... }`
- Good coverage of extraction scenarios
- Clean, focused tests

**Tests**:
```
test_default_config
test_extract_raw_graphql
test_position_from_offset
typescript_tests::test_extract_tagged_template_with_import
typescript_tests::test_extract_tagged_template_without_import_disallowed
typescript_tests::test_extract_tagged_template_without_import_allowed
typescript_tests::test_extract_from_apollo_client
typescript_tests::test_extract_multiple_queries
typescript_tests::test_extract_graphql_tag_identifier
```

**Assessment**: ✅ Good
- Clean organization with nested modules
- No external dependencies needed
- Appropriate inline fixtures

**Recommendations**:
- Keep nested module organization
- Could benefit from snapshot tests for extracted content

---

### graphql-config

**Location**: `crates/graphql-config/src/config.rs:387-500+`

**Current State**:
- No database needed (pure config parsing)
- Good coverage of config variants
- Clean YAML deserialization tests

**Tests**:
```
test_single_project_config
test_multi_project_config
test_schema_config_paths
test_remote_schema_detection
test_documents_config_patterns
test_extensions_field
```

**Assessment**: ✅ Good
- Clean, focused tests
- Appropriate for config crate
- No changes needed

---

### graphql-introspect

**Location**: `crates/graphql-introspect/src/`

**Files with tests**:
- `client.rs` - HTTP client tests
- `query.rs` - introspection query tests
- `sdl.rs` - SDL conversion tests

**Assessment**: ⚠️ Needs Review
- Likely needs mock HTTP responses
- May benefit from fixture files for large introspection results

---

### graphql-cli

**Location**: `crates/graphql-cli/src/commands/`

**Files with tests**:
- `common.rs`
- `deprecations.rs`
- `schema.rs`

**Assessment**: ⚠️ Needs Review
- CLI testing often benefits from integration tests
- Should test command output formatting

---

### graphql-lsp

**Location**: `crates/graphql-lsp/src/workspace.rs`

**Assessment**: ⚠️ Needs Review
- LSP protocol testing is complex
- May need mock client/server infrastructure

---

## Key Findings

### 1. Duplication Analysis

| Duplicated Code | Occurrences | Lines Each |
|-----------------|-------------|------------|
| `TestDatabase` definition | 6 | 15-25 |
| `create_project_files()` | 8+ | 20-30 |
| `TEST_SCHEMA` constants | 3+ | 20-40 |

**Total duplicated lines**: ~300-400 lines

### 2. Pattern Inconsistencies

| Aspect | Variations Found |
|--------|------------------|
| TestDatabase | 3 different implementations |
| Project creation | 4 different function signatures |
| Fixture definition | Inline strings, constants, no pattern |
| Assertion style | Manual only, no snapshots |

### 3. Missing Infrastructure

- No shared test utilities crate
- No snapshot testing framework
- No integration test directories
- No shared fixtures module
- No test documentation standards

---

## Restructuring Plan

### Phase 1: Create Shared Infrastructure

Create `crates/graphql-test-utils/` with:

```
graphql-test-utils/
├── Cargo.toml
└── src/
    ├── lib.rs           # Re-exports
    ├── database.rs      # TestDatabase implementations
    ├── project.rs       # create_project_files(), builders
    ├── fixtures.rs      # Shared large fixtures
    ├── cursor.rs        # extract_cursor() and position helpers
    └── assertions.rs    # Custom assertion helpers
```

### Phase 2: Migrate TestDatabase

Replace all duplicate TestDatabase definitions with imports:

```rust
// Before (in each crate)
#[salsa::db]
#[derive(Clone, Default)]
struct TestDatabase {
    storage: salsa::Storage<Self>,
}
// ... 15 lines of trait impls

// After
use graphql_test_utils::TestDatabase;
```

### Phase 3: Standardize Project Creation

```rust
// Simple function for common cases
let (db, project) = test_project(schema, document);

// Builder for complex scenarios
let project = TestProjectBuilder::new()
    .with_schema("schema.graphql", SCHEMA)
    .with_document("query.graphql", QUERY)
    .with_document("fragments.graphql", FRAGMENTS)
    .with_config(LintConfig { ... })
    .build();
```

### Phase 4: Add Selective Snapshot Testing

Add `insta` for:
- Diagnostic messages
- Error message formatting
- CLI output
- Lint rule messages

```rust
#[test]
fn test_unknown_type_diagnostic() {
    let diagnostics = validate("query { unknownField }");
    insta::assert_snapshot!(format_diagnostics(&diagnostics));
}
```

### Phase 5: Organize Fixtures

```rust
// Small fixtures - inline (contextual)
let schema = "type User { id: ID! }";

// Large fixtures - shared module
use graphql_test_utils::fixtures::{
    STARWARS_SCHEMA,    // Full schema with types
    GITHUB_SCHEMA,      // Complex real-world schema
    BASIC_SCHEMA,       // Minimal Query + User type
};
```

### Phase 6: Add Integration Tests

Create `tests/` directories for crates that need them:

```
crates/graphql-analysis/tests/
└── validation_integration.rs

crates/graphql-ide/tests/
└── ide_features_integration.rs

tests/  # Workspace-level
├── end_to_end.rs
└── multi_project.rs
```

---

## Implementation Phases

### Phase 1: Foundation (graphql-test-utils)

**Priority**: High
**Effort**: Medium

1. Create `crates/graphql-test-utils/Cargo.toml`
2. Implement `TestDatabase` with all trait impls
3. Implement `create_project_files()` and builder
4. Export `extract_cursor()` from graphql-ide
5. Add basic documentation

### Phase 2: Migration (Core Crates)

**Priority**: High
**Effort**: Medium

Order of migration:
1. `graphql-hir` - most complex TestDatabase usage
2. `graphql-analysis` - most duplication
3. `graphql-linter` - standardize rule tests
4. Others as needed

### Phase 3: Snapshot Testing

**Priority**: Medium
**Effort**: Low

1. Add `insta` to dev-dependencies
2. Convert diagnostic message tests
3. Add snapshot review to CI

### Phase 4: Fixtures & Documentation

**Priority**: Medium
**Effort**: Low

1. Create fixtures module with shared schemas
2. Document testing patterns in crate READMEs
3. Add examples of each pattern

### Phase 5: Integration Tests

**Priority**: Low
**Effort**: Medium

1. Add `tests/` directories
2. Write integration tests for public APIs
3. Consider property-based testing for parsing

---

## Shared Infrastructure Design

### TestDatabase

```rust
// crates/graphql-test-utils/src/database.rs

use graphql_db::RootDatabase;
use salsa::Setter;

/// Test database with all GraphQL LSP traits implemented.
/// Use this instead of defining TestDatabase in each test module.
#[salsa::db]
#[derive(Clone, Default)]
pub struct TestDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for TestDatabase {}

#[salsa::db]
impl graphql_db::GraphQLDatabase for TestDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for TestDatabase {}

#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for TestDatabase {}
```

### Project Builder

```rust
// crates/graphql-test-utils/src/project.rs

/// Builder for test projects with multiple files.
pub struct TestProjectBuilder {
    schemas: Vec<(String, String)>,
    documents: Vec<(String, String)>,
    config: Option<LintConfig>,
}

impl TestProjectBuilder {
    pub fn new() -> Self { ... }

    pub fn with_schema(mut self, name: &str, content: &str) -> Self { ... }

    pub fn with_document(mut self, name: &str, content: &str) -> Self { ... }

    pub fn with_config(mut self, config: LintConfig) -> Self { ... }

    /// Build and return (database, project_files)
    pub fn build(self) -> (TestDatabase, ProjectFiles) { ... }
}

/// Simple helper for single schema + document tests
pub fn test_project(schema: &str, document: &str) -> (TestDatabase, ProjectFiles) {
    TestProjectBuilder::new()
        .with_schema("schema.graphql", schema)
        .with_document("query.graphql", document)
        .build()
}
```

### Cursor Extraction

```rust
// crates/graphql-test-utils/src/cursor.rs

use graphql_ide::Position;

/// Extract cursor position from source marked with `*`.
///
/// # Example
/// ```
/// let (source, pos) = extract_cursor("query { user*Name }");
/// assert_eq!(source, "query { userName }");
/// assert_eq!(pos, Position::new(0, 12));
/// ```
pub fn extract_cursor(input: &str) -> (String, Position) { ... }
```

### Shared Fixtures

```rust
// crates/graphql-test-utils/src/fixtures.rs

/// Basic schema with Query and User types
pub const BASIC_SCHEMA: &str = r#"
type Query {
    user(id: ID!): User
    users: [User!]!
}

type User {
    id: ID!
    name: String!
    email: String!
}
"#;

/// Schema with nested types for testing recursion
pub const NESTED_SCHEMA: &str = r#"
type Query {
    user(id: ID!): User
}

type User {
    id: ID!
    name: String!
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
    author: User!
    comments: [Comment!]!
}

type Comment {
    id: ID!
    text: String!
    author: User!
}
"#;

// Additional fixtures...
```

---

## Pattern Reference

### Pattern 1: Simple Unit Test

```rust
use graphql_test_utils::{test_project, BASIC_SCHEMA};

#[test]
fn test_valid_query() {
    let (db, project) = test_project(
        BASIC_SCHEMA,
        "query { user(id: \"1\") { id name } }",
    );

    let diagnostics = validate_file(&db, ...);
    assert!(diagnostics.is_empty());
}
```

### Pattern 2: Complex Multi-File Test

```rust
use graphql_test_utils::TestProjectBuilder;

#[test]
fn test_fragment_across_files() {
    let (db, project) = TestProjectBuilder::new()
        .with_schema("schema.graphql", NESTED_SCHEMA)
        .with_document("fragments.graphql", "fragment UserFields on User { id name }")
        .with_document("query.graphql", "query { user { ...UserFields } }")
        .build();

    // Test cross-file fragment resolution
    let diagnostics = validate_file(&db, ...);
    assert!(diagnostics.is_empty());
}
```

### Pattern 3: Position-Based Test (IDE Features)

```rust
use graphql_test_utils::extract_cursor;

#[test]
fn test_goto_definition_on_fragment_spread() {
    let (source, cursor) = extract_cursor(r#"
        query {
            user {
                *...UserFields
            }
        }
    "#);

    let result = analysis.goto_definition(&path, cursor);
    assert!(result.is_some());
}
```

### Pattern 4: Snapshot Test (Diagnostics)

```rust
#[test]
fn test_unknown_field_diagnostic_message() {
    let (db, project) = test_project(
        BASIC_SCHEMA,
        "query { user { unknownField } }",
    );

    let diagnostics = validate_file(&db, ...);
    insta::assert_snapshot!(format_diagnostics(&diagnostics));
}
```

### Pattern 5: Incremental Computation Verification

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn test_caching_behavior() {
    CALL_COUNT.store(0, Ordering::SeqCst);

    // First call - should compute
    let _ = some_query(&db, ...);
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);

    // Second call - should cache
    let _ = some_query(&db, ...);
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1, "Should use cache");
}
```

---

## Success Metrics

After restructuring:

| Metric | Before | After |
|--------|--------|-------|
| Duplicated TestDatabase lines | ~120 | 0 |
| Duplicated create_project_files lines | ~200 | 0 |
| Lines in shared infrastructure | 0 | ~300 |
| Net lines changed | - | ~-20 |
| Time to write new test | Variable | Consistent |
| Test readability | Inconsistent | Uniform |

---

## Next Steps

1. [x] Create `graphql-test-utils` crate
2. [x] Implement core TestDatabase
3. [x] Implement project builder
4. [x] Migrate graphql-hir tests
5. [x] Migrate graphql-analysis tests
6. [ ] Add insta dependency
7. [ ] Convert diagnostic tests to snapshots
8. [x] Create shared fixtures
9. [ ] Add integration test directories
10. [x] Update CLAUDE.md with testing guidelines

---

## Appendix: Files to Modify

### New Files

- `crates/graphql-test-utils/Cargo.toml`
- `crates/graphql-test-utils/src/lib.rs`
- `crates/graphql-test-utils/src/database.rs`
- `crates/graphql-test-utils/src/project.rs`
- `crates/graphql-test-utils/src/cursor.rs`
- `crates/graphql-test-utils/src/fixtures.rs`
- `crates/graphql-test-utils/src/assertions.rs`

### Files to Update

- `crates/graphql-hir/src/lib.rs` (remove TestDatabase, use shared)
- `crates/graphql-analysis/src/lib.rs` (remove TestDatabase)
- `crates/graphql-analysis/src/validation.rs` (remove TestDatabase, create_project_files)
- `crates/graphql-analysis/src/document_validation.rs` (remove TestDatabase, create_project_files)
- `crates/graphql-linter/src/rules/*.rs` (standardize helpers)
- `Cargo.toml` (add graphql-test-utils to workspace)
- `.claude/CLAUDE.md` (add testing guidelines)

---

## Implementation Notes

### Difficulties Encountered

#### Cyclic Dependency Constraints

The most significant challenge in implementing shared test infrastructure was **cyclic dev-dependencies** in Cargo.

**Problem**: When `graphql-test-utils` depends on `graphql-analysis` (to implement `GraphQLAnalysisDatabase` on `TestDatabase`), and `graphql-analysis` then adds `graphql-test-utils` as a dev-dependency, Cargo creates a cyclic dependency. This causes version conflicts:

```
error: failed to select a version for `graphql-hir`
required by package `graphql-test-utils`
multiple different versions of crate `graphql_hir` in the dependency graph
```

**Solution**: Made `graphql-analysis` an optional dependency via a feature flag:

```toml
[features]
default = []
analysis = ["dep:graphql-analysis"]
```

Crates use this accordingly:
- **graphql-hir, graphql-analysis**: Keep their own local `TestDatabase` (can't use the shared one due to cycles)
- **graphql-linter, graphql-ide**: Use `graphql-test-utils` without the `analysis` feature
- **Higher-level crates**: Can use `graphql-test-utils` with `analysis` feature if they don't depend on analysis

#### Immutable vs Mutable Database References

**Problem**: The shared `create_project_files()` helper requires `&mut db` (to create Salsa inputs), but most test code used `&db`.

**Solution**: Updated all test functions to use `let mut db` instead of `let db`. This is a mechanical change but required touching every test.

#### TestDatabase Specializations

**Problem**: Some crates need specialized `TestDatabase` implementations. For example, `document_validation.rs` stores `project_files` in a `Cell` to implement `GraphQLHirDatabase::project_files()`.

**Solution**: Keep specialized `TestDatabase` implementations where needed. The shared infrastructure provides a baseline, not a one-size-fits-all solution.

### Design Decisions

1. **`graphql_db::test_utils` module**: Put basic helpers (`create_project_files`) in the foundation crate (`graphql-db`) with a `test-utils` feature flag. This avoids cycles for low-level crates.

2. **`graphql-test-utils` crate**: Higher-level utilities (builders, fixtures, cursor extraction) go in a dedicated crate that can depend on multiple layers.

3. **Feature-gated analysis support**: The `analysis` feature on `graphql-test-utils` enables `GraphQLAnalysisDatabase` impl. Only enable when needed and when dependency cycles aren't a concern.

---

## Testing Strategy Reference

### When to Use Which Testing Pattern

| Crate Layer | TestDatabase Source | ProjectFiles Helper |
|-------------|---------------------|---------------------|
| graphql-db | `RootDatabase` directly | `test_utils::create_project_files` |
| graphql-syntax | None needed (standalone parsing) | N/A |
| graphql-hir | Local `TestDatabase` | `graphql_db::test_utils::create_project_files` |
| graphql-analysis | Local `TestDatabase` | `graphql_db::test_utils::create_project_files` |
| graphql-linter | `graphql_test_utils::TestDatabase` | `graphql_test_utils::TestProjectBuilder` |
| graphql-ide | `graphql_test_utils::TestDatabase` (with analysis) | `graphql_test_utils::TestProjectBuilder` |

### Key Principles

1. **Use the shared infrastructure when possible**: Reduces duplication and ensures consistency
2. **Keep local TestDatabase when cycles prevent sharing**: Add a comment explaining why
3. **Prefer `TestProjectBuilder` for multi-file tests**: More readable than manual file setup
4. **Use inline fixtures for small tests**: `test_project(schema, doc)` is fine for simple cases
5. **Use shared fixtures for large/complex schemas**: Avoids duplication across tests

### Example: Writing a New Test

```rust
// Simple single-file test
#[test]
fn test_validates_field() {
    let (db, project) = test_project(
        "type Query { user: User } type User { id: ID! }",
        "query { user { invalidField } }",
    );
    // ... assertions
}

// Multi-file test with fragments
#[test]
fn test_cross_file_fragment() {
    let (db, project) = TestProjectBuilder::new()
        .with_schema("schema.graphql", NESTED_SCHEMA)
        .with_document("fragments.graphql", "fragment UserFields on User { id }")
        .with_document("query.graphql", "query { user { ...UserFields } }")
        .build();
    // ... assertions
}

// IDE feature test with cursor
#[test]
fn test_goto_definition() {
    let (source, pos) = extract_cursor("query { user { *name } }");
    let (db, project) = test_project(BASIC_SCHEMA, &source);
    let result = goto_definition(&db, "query.graphql", pos);
    // ... assertions
}
```

### Future Considerations

1. **Snapshot testing**: Consider `insta` for complex diagnostic output validation
2. **Property-based testing**: Consider `proptest` for parser edge cases
3. **Per-crate integration tests**: Add `tests/` directories for public API testing
4. **Benchmark tests**: Use Criterion for performance regression testing
