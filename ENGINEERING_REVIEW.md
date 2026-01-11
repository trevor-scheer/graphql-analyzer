# Engineering Review: GraphQL LSP

**Author:** Distinguished Engineer Review
**Date:** January 2026
**Status:** Comprehensive Analysis

---

## Executive Summary

This document provides a critical engineering review of the GraphQL LSP project, examining architecture, implementation patterns, and areas for improvement. The project demonstrates strong foundations with its Salsa-based incremental computation model but has several areas requiring attention for production readiness.

**Overall Assessment:** The architecture is sound and follows rust-analyzer patterns appropriately. However, there are concerns around concurrency patterns, error handling consistency, API surface exposure, and some missed optimization opportunities in the query design.

---

## Table of Contents

1. [Architecture Critiques](#1-architecture-critiques)
2. [Concurrency and Thread Safety](#2-concurrency-and-thread-safety)
3. [Query Design and Incrementality](#3-query-design-and-incrementality)
4. [Error Handling](#4-error-handling)
5. [LSP Implementation](#5-lsp-implementation)
6. [API Design and Type Safety](#6-api-design-and-type-safety)
7. [Testing and Reliability](#7-testing-and-reliability)
8. [Performance Considerations](#8-performance-considerations)
9. [Code Quality and Maintainability](#9-code-quality-and-maintainability)

---

## 1. Architecture Critiques

### 1.1 RefCell Usage in IdeDatabase Violates Thread Safety

**Problem Statement:**
The `IdeDatabase` struct uses `std::cell::RefCell` for `lint_config`, `extract_config`, and `project_files`:

```rust
struct IdeDatabase {
    storage: salsa::Storage<Self>,
    lint_config: std::cell::RefCell<Arc<graphql_linter::LintConfig>>,
    extract_config: std::cell::RefCell<Arc<graphql_extract::ExtractConfig>>,
    project_files: std::cell::RefCell<Option<graphql_db::ProjectFiles>>,
}
```

`RefCell` provides interior mutability but is NOT thread-safe. This creates a potential panic if `IdeDatabase` is accessed from multiple threads, which is likely in an async LSP context. The `AnalysisHost` wraps this in a `RwLock`, but if any code path creates `Analysis` snapshots and queries concurrently, this will panic.

**Agents Consulted:** rust-analyzer, rust

**Proposed Solution:**
Replace `RefCell` with thread-safe alternatives:

```rust
struct IdeDatabase {
    storage: salsa::Storage<Self>,
    lint_config: Arc<RwLock<Arc<graphql_linter::LintConfig>>>,
    extract_config: Arc<RwLock<Arc<graphql_extract::ExtractConfig>>>,
    project_files: Arc<RwLock<Option<graphql_db::ProjectFiles>>>,
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Use `parking_lot::RwLock`** | Non-poisoning, faster than std | Adds dependency (already used) |
| **Use `Arc<AtomicRefCell>`** | Explicit thread-safety with borrow checking | Less common pattern |
| **Make configs immutable Salsa inputs** | Proper incremental semantics | More invasive change |

**Recommendation:** Use `parking_lot::RwLock` (already a dependency) for consistency with `AnalysisHost`. Long-term, consider making configs proper Salsa inputs for better incrementality.

---

### 1.2 Dependency on Forked apollo-rs Creates Maintenance Burden

**Problem Statement:**
The project depends on a fork of apollo-rs from `https://github.com/trevor-scheer/apollo-rs.git` branch `parse_with_offset`. This fork adds `ExecutableDocument::builder()` and related APIs not yet in upstream.

```toml
apollo-compiler = { git = "https://github.com/trevor-scheer/apollo-rs.git", branch = "parse_with_offset" }
```

This creates:
1. Maintenance burden to keep fork synchronized
2. Inability to receive upstream security/bug fixes automatically
3. Potential for divergence making upstream contribution harder

**Agents Consulted:** apollo-rs, rust

**Proposed Solution:**
Work with Apollo team to upstream the required APIs. The `ExecutableDocument::builder()` pattern is valuable for incremental document construction.

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Upstream the changes** | Removes fork, benefits ecosystem | Requires Apollo team buy-in |
| **Abstract behind trait** | Allows swapping implementations | Adds indirection |
| **Document dependency clearly** | Transparency | Still has maintenance burden |

**Recommendation:** Prioritize upstreaming. In the interim, add CI job to track upstream changes and document the fork clearly.

---

### 1.3 Layer Boundary Violations in graphql-ide

**Problem Statement:**
The `graphql-ide` crate, which should be a thin translation layer between analysis and LSP, contains significant logic:

1. `lib.rs` is 179KB (approximately 5000+ lines)
2. `symbol.rs` is 61KB
3. Contains its own `IdeDatabase` that wraps analysis database

The crate has grown beyond its intended scope as a "POD types + thin API" layer. It now contains:
- Full document symbol extraction logic
- Symbol search implementation
- Completion logic
- Hover implementation

This makes testing harder and violates the layered architecture principle.

**Agents Consulted:** rust-analyzer, lsp

**Proposed Solution:**
Split `graphql-ide` into:
1. `graphql-ide-types` - POD types only (Position, Range, Location, etc.)
2. `graphql-ide` - Thin translation calling analysis queries
3. Move heavy logic to `graphql-analysis` as proper Salsa queries

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Split into multiple crates** | Clean separation | More crates to manage |
| **Make symbol logic Salsa queries** | Cacheable, incremential | Requires analysis changes |
| **Keep but document boundaries** | Least invasive | Technical debt remains |

**Recommendation:** Extract POD types first, then progressively move logic to proper queries.

---

## 2. Concurrency and Thread Safety

### 2.1 AnalysisHost Lock Granularity

**Problem Statement:**
The `AnalysisHost` uses a single `RwLock<IdeDatabase>` for the entire database:

```rust
pub struct AnalysisHost {
    db: Arc<RwLock<IdeDatabase>>,
    // ...
}
```

All operations require acquiring this lock, which creates contention. In the LSP server, every `did_change`, `did_open`, completion request, hover request, etc. must acquire this lock.

The LSP server compounds this by using `Mutex<AnalysisHost>`:

```rust
hosts: Arc<DashMap<(String, String), Arc<Mutex<AnalysisHost>>>>
```

This creates nested locking: acquire `DashMap` entry, then `Mutex<AnalysisHost>`, then potentially `RwLock<IdeDatabase>`.

**Agents Consulted:** rust-analyzer, rust, lsp

**Proposed Solution:**
Adopt rust-analyzer's pattern more faithfully:

1. `AnalysisHost` applies changes and creates `Analysis` snapshots
2. `Analysis` is `Send + Sync` and can be queried without locks
3. Snapshots are immutable - no locking needed for queries

```rust
impl AnalysisHost {
    pub fn snapshot(&self) -> Analysis {
        let db = self.db.read().clone();  // Salsa clone is cheap
        Analysis { db }
    }
}

impl Analysis {
    pub fn diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
        // No locking! Database is owned, queries are pure
    }
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Proper snapshot pattern** | Lock-free queries, matches rust-analyzer | Requires Salsa db cloning |
| **Fine-grained locking** | Reduces contention | Complex, error-prone |
| **Read-write split** | Separate read/write paths | Still requires locking |

**Recommendation:** Implement proper snapshot pattern. Salsa databases are designed for cheap cloning.

---

### 2.2 Async Lock Holding Across Await Points

**Problem Statement:**
The LSP server holds `Mutex<AnalysisHost>` locks across async await points:

```rust
async fn load_all_project_files(&self, ...) {
    // ...
    for pattern in patterns {
        for entry in paths {
            // Lock held across entire iteration including file I/O
            let mut host_guard = host.lock().await;
            host_guard.add_file(...);
            drop(host_guard);  // Explicit drop doesn't help - reacquired
        }
    }
}
```

This pattern can cause deadlocks and severely limits concurrency. File I/O is slow; holding a lock during it blocks all other operations.

**Agents Consulted:** rust, lsp

**Proposed Solution:**
Batch file additions:

```rust
async fn load_all_project_files(&self, ...) {
    // Phase 1: Collect all files (no lock)
    let files: Vec<(FilePath, String, FileKind)> = collect_all_files().await;

    // Phase 2: Add to host in one batch (single lock acquisition)
    {
        let mut host = self.hosts.get(&key).lock().await;
        for (path, content, kind) in files {
            host.add_file(&path, &content, kind, 0);
        }
        host.rebuild_project_files();
    }
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Batch updates** | Single lock acquisition | Memory usage for collecting |
| **Background task** | Non-blocking | Complex coordination |
| **Lock-free channel** | No locks during I/O | Requires architecture change |

**Recommendation:** Implement batched updates immediately; consider background loading for large projects.

---

## 3. Query Design and Incrementality

### 3.1 FileEntryMap Invalidation Pattern

**Problem Statement:**
The current `FileEntryMap` design wraps a `HashMap<FileId, FileEntry>` in a single Salsa input:

```rust
#[salsa::input]
pub struct FileEntryMap {
    pub entries: Arc<HashMap<FileId, FileEntry>>,
}
```

While individual `FileEntry` instances can be updated, queries that need to iterate over all files depend on `FileEntryMap` itself. This causes broader invalidation than necessary.

For example, `schema_types` iterates over all schema files:

```rust
pub fn schema_types(db, project_files) -> Arc<HashMap<Arc<str>, TypeDef>> {
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    for file_id in schema_ids.iter() {
        if let Some((content, metadata)) = file_lookup(db, project_files, *file_id) {
            // Per-file query
        }
    }
}
```

This is good for per-file caching but the aggregate query still depends on `SchemaFileIds`.

**Agents Consulted:** rust-analyzer, salsa (via rust-analyzer agent)

**Proposed Solution:**
Consider using Salsa's `#[salsa::accumulator]` pattern for collecting results:

```rust
#[salsa::accumulator]
pub struct TypeDefAccumulator(TypeDef);

#[salsa::tracked]
fn collect_types(db, file_id, content, metadata) {
    // Emit type defs via accumulator
    for type_def in extract_types(...) {
        TypeDefAccumulator::push(db, type_def);
    }
}
```

This allows Salsa to track fine-grained dependencies.

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Accumulators** | Fine-grained invalidation | Learning curve |
| **Interned keys** | O(1) lookup by ID | Requires ID generation |
| **Current approach** | Simple, works | Coarser invalidation |

**Recommendation:** Current approach is acceptable for MVP. Profile before optimizing.

---

### 3.2 Duplicate Parsing in Validation Flow

**Problem Statement:**
The validation flow parses documents multiple times:

1. `graphql_syntax::parse()` parses the document
2. `collect_referenced_fragments_transitive()` uses the parsed tree
3. `apollo_compiler::parser::Parser::new().parse_into_executable_builder()` re-parses fragment sources

Step 3 re-parses fragment source text that was already parsed:

```rust
if let Some(fragment_source) = graphql_hir::fragment_source(db, project_files, key) {
    apollo_compiler::parser::Parser::new().parse_into_executable_builder(
        fragment_source.as_ref(),
        format!("fragment:{fragment_name}"),
        &mut builder,
    );
}
```

**Agents Consulted:** apollo-rs, rust-analyzer

**Proposed Solution:**
Cache parsed AST for fragments and use `builder.add_ast_document()`:

```rust
// In graphql-hir
#[salsa::tracked]
pub fn fragment_ast(db, project_files, fragment_name: Arc<str>) -> Option<Arc<apollo_compiler::ast::Document>> {
    // Return cached AST instead of source
}

// In validation
if let Some(frag_ast) = graphql_hir::fragment_ast(db, project_files, key) {
    builder.add_ast_document(&frag_ast, false);
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Cache fragment AST** | No re-parsing | More memory |
| **Single combined document** | One parse | Loses source file info |
| **Accept current behavior** | Simple | Performance cost |

**Recommendation:** Implement fragment AST caching; it's consistent with the incremental architecture.

---

## 4. Error Handling

### 4.1 Inconsistent Error Types Across Crates

**Problem Statement:**
Error handling varies significantly across crates:

- `graphql-config`: Uses `thiserror` with custom `ConfigError`
- `graphql-extract`: Uses `thiserror` with custom `ExtractionError`
- `graphql-introspect`: Uses `thiserror` with custom `IntrospectionError`
- `graphql-ide`: Uses `anyhow::Result` in some places
- `graphql-lsp`: Mix of `anyhow` and `tower_lsp::jsonrpc::Result`
- `graphql-analysis`: Returns `Arc<Vec<Diagnostic>>` (not `Result`)

This inconsistency makes error propagation awkward and error handling patterns hard to follow.

**Agents Consulted:** rust

**Proposed Solution:**
Establish consistent error handling strategy:

1. **Library crates** (db, syntax, hir, analysis, linter): Use `thiserror` with crate-specific error types
2. **Application boundaries** (lsp, cli): Use `anyhow` for convenience
3. **IDE layer**: Define clear error types for IDE operations

```rust
// graphql-ide/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum IdeError {
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("analysis failed: {0}")]
    AnalysisFailed(String),
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **thiserror everywhere** | Typed errors, no allocations | More boilerplate |
| **anyhow everywhere** | Convenient | Loses type information |
| **Mixed approach** | Pragmatic | Current inconsistency |

**Recommendation:** Adopt the proposed layered approach; it balances type safety with convenience.

---

### 4.2 Silent Error Swallowing in Extraction

**Problem Statement:**
GraphQL extraction from TypeScript/JavaScript silently swallows errors:

```rust
fn extract_and_parse(db, content, uri) -> Parse {
    let extracted = match extract_from_source(content, language, &config) {
        Ok(blocks) => blocks,
        Err(e) => {
            tracing::error!(error = ?e, "Extraction failed");
            Vec::new()  // Silent fallback to empty!
        }
    };
    // ...
}
```

When extraction fails, users see no errors in their editor. They might have malformed JavaScript that prevents GraphQL extraction, but they receive no feedback.

**Agents Consulted:** graphql, lsp

**Proposed Solution:**
Return extraction errors as diagnostics:

```rust
pub struct Parse {
    pub tree: Arc<SyntaxTree>,
    pub ast: Arc<Document>,
    pub blocks: Vec<ExtractedBlock>,
    pub errors: Vec<ParseError>,
    pub extraction_errors: Vec<ExtractionError>,  // NEW
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Return errors as diagnostics** | User sees issues | More diagnostic types |
| **Log warnings to client** | Non-intrusive | Easy to miss |
| **Fail loudly** | Forces resolution | Bad UX |

**Recommendation:** Add extraction errors to diagnostic output; users deserve visibility.

---

## 5. LSP Implementation

### 5.1 Full Text Sync Instead of Incremental

**Problem Statement:**
The LSP server uses full document sync:

```rust
TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)
```

Every keystroke sends the entire document content. For large files, this wastes bandwidth and CPU. The comment in the SME agent explicitly states:

> Use incremental text sync, not full sync.

**Agents Consulted:** lsp

**Proposed Solution:**
Implement incremental text synchronization:

```rust
text_document_sync: Some(TextDocumentSyncCapability::Options(
    TextDocumentSyncOptions {
        open_close: Some(true),
        change: Some(TextDocumentSyncKind::INCREMENTAL),
        // ...
    }
)),
```

Then apply incremental changes to the internal document state.

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Incremental sync** | Efficient, standard | More implementation work |
| **Full sync + debouncing** | Simple | Still sends full doc |
| **Full sync (current)** | Trivial | Inefficient |

**Recommendation:** Implement incremental sync; it's expected for production LSP servers.

---

### 5.2 No Request Cancellation Support

**Problem Statement:**
The LSP server doesn't implement request cancellation. When a user types quickly, each keystroke can trigger:
- `textDocument/didChange`
- Validation
- Potentially hover/completion requests

Without cancellation, stale requests continue executing even after new input arrives. This wastes resources and can cause outdated results to appear.

**Agents Consulted:** lsp, rust-analyzer

**Proposed Solution:**
Implement cooperative cancellation using `tokio::select!` or a cancellation token:

```rust
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let cancel_token = self.cancellation.token_for_request(request_id);

    tokio::select! {
        result = self.compute_hover(params) => result,
        _ = cancel_token.cancelled() => Err(Error::request_cancelled()),
    }
}
```

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **tokio::select cancellation** | Clean, idiomatic | Requires cancel tokens |
| **Salsa cancellation** | Integrated with queries | Complex setup |
| **Ignore (current)** | Simple | Wastes resources |

**Recommendation:** Implement cancellation; it's critical for responsiveness.

---

### 5.3 Missing Document Version Tracking

**Problem Statement:**
The LSP server receives document versions but doesn't track them:

```rust
async fn did_change(&self, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri;
    // params.text_document.version is ignored!
    for change in params.content_changes { ... }
}
```

Document versions are crucial for:
1. Detecting out-of-order updates
2. Correlating diagnostics with document state
3. Avoiding race conditions

**Agents Consulted:** lsp

**Proposed Solution:**
Track document versions in `AnalysisHost`:

```rust
pub struct AnalysisHost {
    db: Arc<RwLock<IdeDatabase>>,
    document_versions: HashMap<FilePath, i32>,
}

pub fn update_file(&mut self, path: &FilePath, content: &str, version: i32) {
    if let Some(&current) = self.document_versions.get(path) {
        if version <= current {
            tracing::warn!("Ignoring stale update: {} <= {}", version, current);
            return;
        }
    }
    self.document_versions.insert(path.clone(), version);
    // ... actual update
}
```

**Recommendation:** Implement version tracking; it's required for correctness.

---

## 6. API Design and Type Safety

### 6.1 FileId Lacks Type Safety

**Problem Statement:**
`FileId` is a simple newtype wrapper around `u32`:

```rust
pub struct FileId(u32);
```

Nothing prevents confusing a `FileId` from one database with another, or using an invalid ID. The ID is also publicly constructible with `FileId::new(42)`, which could create invalid references.

**Agents Consulted:** rust

**Proposed Solution:**
Use Salsa's interning for proper FileId management:

```rust
#[salsa::interned]
pub struct FileId<'db> {
    pub path: Arc<str>,
}
```

This provides:
1. Type-safe IDs tied to database lifetime
2. Automatic deduplication
3. Impossible to create invalid IDs

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Salsa interning** | Type-safe, automatic | Lifetime complexity |
| **GenerationalArena** | Detects use-after-free | Additional dependency |
| **Current approach** | Simple | No safety guarantees |

**Recommendation:** Salsa interning is the idiomatic solution for this architecture.

---

### 6.2 Public Fields Enable Invalid State

**Problem Statement:**
Many types have public fields that could be put into invalid states:

```rust
pub struct TypeRef {
    pub name: Arc<str>,
    pub is_list: bool,
    pub is_non_null: bool,
    pub inner_non_null: bool,  // Only valid if is_list is true
}
```

The comment "Only valid if is_list is true" indicates an invariant that the type system doesn't enforce.

**Agents Consulted:** rust

**Proposed Solution:**
Use the type-state pattern or smart constructors:

```rust
pub enum TypeRef {
    Named { name: Arc<str>, non_null: bool },
    List { inner: Box<TypeRef>, non_null: bool },
}
```

This makes invalid states unrepresentable.

**Alternatives:**

| Alternative | Pros | Cons |
|-------------|------|------|
| **Enum encoding** | Invalid states unrepresentable | Breaking change |
| **Smart constructors** | Validates on construction | Still allows invalid via fields |
| **Document invariants** | No code change | Bugs possible |

**Recommendation:** For HIR types, enum encoding is worth the breaking change. For POD types in graphql-ide, documentation may suffice.

---

## 7. Testing and Reliability

### 7.1 Placeholder Assertions in Tests

**Problem Statement:**
Some tests have placeholder assertions that always pass:

```rust
#[test]
fn test_file_diagnostics_empty() {
    // ...
    let diagnostics = file_diagnostics(&db, content, metadata, None);

    // Should have no diagnostics for valid schema
    // Note: This will work once we implement the parse query properly
    assert!(diagnostics.is_empty() || !diagnostics.is_empty()); // ALWAYS TRUE!
}
```

This assertion provides no value and masks potential regressions.

**Agents Consulted:** rust

**Proposed Solution:**
Fix or remove placeholder tests:

```rust
#[test]
fn test_file_diagnostics_empty() {
    let diagnostics = file_diagnostics(&db, content, metadata, None);
    assert!(
        diagnostics.is_empty(),
        "Valid schema should have no diagnostics, got: {:?}",
        diagnostics
    );
}
```

**Recommendation:** Audit all tests for placeholder assertions; they indicate incomplete implementation.

---

### 7.2 Missing Integration Tests for Cross-File Scenarios

**Problem Statement:**
Most tests are unit tests within individual crates. Cross-crate integration tests exist in `tests/` but coverage of complex scenarios is limited:

1. Fragment resolution across multiple files
2. Schema extension merging
3. Circular fragment detection
4. TypeScript/JavaScript extraction edge cases

**Agents Consulted:** graphql

**Proposed Solution:**
Add integration test suite covering:

```rust
// tests/cross_file_fragments.rs
#[test]
fn fragment_defined_in_one_file_used_in_another() { ... }

#[test]
fn transitive_fragment_dependencies() { ... }

#[test]
fn circular_fragment_detection() { ... }

#[test]
fn fragment_in_typescript_template_literal() { ... }
```

**Recommendation:** Before 1.0, comprehensive integration test coverage is essential.

---

## 8. Performance Considerations

### 8.1 Unbounded HashMap Growth in Aggregate Queries

**Problem Statement:**
Aggregate queries like `schema_types` and `all_fragments` collect results into `HashMap`s without size limits:

```rust
pub fn schema_types(db, project_files) -> Arc<HashMap<Arc<str>, TypeDef>> {
    let mut types = HashMap::new();
    for file_id in schema_ids.iter() {
        // Could be thousands of types
        for type_def in file_types.iter() {
            types.insert(type_def.name.clone(), type_def.clone());
        }
    }
    Arc::new(types)
}
```

For large schemas (Apollo Federation schemas can have 10,000+ types), this creates memory pressure.

**Agents Consulted:** rust-analyzer

**Proposed Solution:**
Consider lazy iteration or bounded caching:

```rust
// Option 1: Return iterator instead of materialized collection
pub fn schema_type_names(db, project_files) -> impl Iterator<Item = Arc<str>> { ... }

// Option 2: Add size hints and capacity
let mut types = HashMap::with_capacity(estimated_type_count);
```

**Recommendation:** Monitor memory usage with large schemas; optimize if needed.

---

### 8.2 String Cloning in Hot Paths

**Problem Statement:**
Many hot paths clone strings instead of using references:

```rust
for fragment_name in &referenced_fragments {
    let key: Arc<str> = Arc::from(fragment_name.as_str());  // Creates new Arc!
    // ...
}
```

When `fragment_name` is already an `Arc<str>`, the conversion creates a new allocation.

**Agents Consulted:** rust

**Proposed Solution:**
Use consistent string types:

```rust
// Use Arc<str> consistently
fn collect_referenced_fragments(...) -> HashSet<Arc<str>> { ... }

// Then no conversion needed
for fragment_name in &referenced_fragments {
    if let Some(source) = fragment_source(db, project_files, fragment_name.clone()) {
        // ...
    }
}
```

**Recommendation:** Audit hot paths for unnecessary string conversions.

---

## 9. Code Quality and Maintainability

### 9.1 Large Files Violate Single Responsibility

**Problem Statement:**
Several files are excessively large:

- `graphql-ide/src/lib.rs`: ~5000 lines
- `graphql-ide/src/symbol.rs`: ~1800 lines
- `graphql-lsp/src/server.rs`: ~1300 lines
- `graphql-analysis/src/validation.rs`: ~800 lines

Large files are harder to navigate, test, and maintain.

**Agents Consulted:** rust

**Proposed Solution:**
Split into focused modules:

```
graphql-ide/src/
├── lib.rs              # Public exports only
├── host.rs             # AnalysisHost
├── analysis.rs         # Analysis snapshot
├── diagnostics.rs      # Diagnostic queries
├── completion.rs       # Completion logic
├── hover.rs            # Hover logic
├── goto_definition.rs  # Navigation
├── references.rs       # Find references
├── symbols/
│   ├── mod.rs
│   ├── document.rs
│   └── workspace.rs
└── types.rs            # POD types
```

**Recommendation:** Refactor incrementally; start with extracting independent features.

---

### 9.2 TODO Comments Indicate Incomplete Features

**Problem Statement:**
Multiple TODO comments indicate incomplete implementations:

```rust
// TODO: Extract actual positions from AST nodes
FragmentStructure {
    name_range: name_range(&frag.name),
    type_condition_range: name_range(&frag.type_condition),
    fragment_range: node_range(frag),
}
```

Some TODOs have been present across multiple commits, indicating stalled work.

**Agents Consulted:** N/A

**Proposed Solution:**
1. Create GitHub issues for each TODO
2. Prioritize and schedule
3. Remove TODOs that won't be addressed (document limitations instead)

**Recommendation:** Track TODOs as issues; dead comments accumulate.

---

### 9.3 Missing Module-Level Documentation

**Problem Statement:**
Most modules lack comprehensive documentation:

```rust
// graphql-analysis/src/lib.rs
// GraphQL Analysis Layer
// This crate provides validation and linting on top of the HIR layer.
// All validation is query-based for automatic incrementality via Salsa.
```

This is better than nothing but doesn't explain:
- How to use the crate
- Key types and their relationships
- Error handling patterns
- Examples

**Agents Consulted:** rust

**Proposed Solution:**
Add rustdoc documentation with examples:

```rust
//! # graphql-analysis
//!
//! This crate provides GraphQL validation and linting.
//!
//! ## Architecture
//!
//! Built on top of `graphql-hir`, this layer provides:
//! - Schema validation via [`validate_schema_file`]
//! - Document validation via [`validate_file`]
//! - Lint rule integration via [`lint_file`]
//!
//! ## Example
//!
//! ```rust
//! use graphql_analysis::file_diagnostics;
//!
//! let diagnostics = file_diagnostics(&db, content, metadata, Some(project_files));
//! ```
```

**Recommendation:** Documentation is a feature; prioritize for public APIs.

---

## Summary of Recommendations

### Critical (Address Before Production)

1. **Replace `RefCell` with thread-safe alternatives** (Section 1.1)
2. **Implement proper cancellation** (Section 5.2)
3. **Track document versions** (Section 5.3)
4. **Fix placeholder test assertions** (Section 7.1)

### High Priority (Address Soon)

5. **Upstream apollo-rs fork** (Section 1.2)
6. **Implement incremental text sync** (Section 5.1)
7. **Batch file loading to reduce lock contention** (Section 2.2)
8. **Cache fragment AST to avoid re-parsing** (Section 3.2)

### Medium Priority (Address Before 1.0)

9. **Split graphql-ide into smaller modules** (Section 1.3, 9.1)
10. **Standardize error handling** (Section 4.1)
11. **Surface extraction errors to users** (Section 4.2)
12. **Add integration tests for cross-file scenarios** (Section 7.2)

### Low Priority (Nice to Have)

13. **Adopt Salsa interning for FileId** (Section 6.1)
14. **Use type-state for TypeRef** (Section 6.2)
15. **Add module documentation** (Section 9.3)
16. **Convert TODOs to issues** (Section 9.2)

---

## Appendix: Agents Consulted

| Agent | Areas Consulted |
|-------|-----------------|
| **rust-analyzer** | Query design, incrementality, AnalysisHost pattern, cancellation |
| **rust** | Error handling, thread safety, type design, API patterns |
| **lsp** | Protocol compliance, text sync, cancellation, document lifecycle |
| **graphql** | Spec compliance, fragment scoping, validation behavior |
| **apollo-rs** | Parser usage, CST vs AST, validation API |

---

*This review was conducted using systematic analysis of the codebase and consultation with domain-specific Subject Matter Expert agents. All critiques include problem statements, proposed solutions, and alternatives with pros/cons for informed decision-making.*
