# Performance Analysis: Slowest Code Paths

**Date**: 2026-01-09
**Author**: Performance Engineering Analysis
**Status**: Active

This document identifies the slowest code paths in the GraphQL LSP when operating within large codebases (1,000-50,000+ GraphQL files), provides context for each issue, proposes solutions, and lists alternatives considered.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Issue 1: Synchronous File Loading at Initialization](#issue-1-synchronous-file-loading-at-initialization)
3. [Issue 2: Non-Incremental Apollo-Compiler Validation](#issue-2-non-incremental-apollo-compiler-validation)
4. [Issue 3: Transitive Fragment Resolution](#issue-3-transitive-fragment-resolution)
5. [Issue 4: Linear Workspace/Project Lookup](#issue-4-linear-workspaceproject-lookup)
6. [Issue 5: Full Text Document Sync](#issue-5-full-text-document-sync)
7. [Issue 6: Workspace Symbol Search](#issue-6-workspace-symbol-search)
8. [Issue 7: Schema Merging on Large Schemas](#issue-7-schema-merging-on-large-schemas)
9. [Existing Optimizations (Strengths)](#existing-optimizations-strengths)
10. [Recommended Priority Order](#recommended-priority-order)

---

## Executive Summary

The GraphQL LSP architecture is fundamentally sound with excellent Salsa integration for incremental computation. The main performance concerns for large codebases are:

| Priority | Issue                          | Impact                            | Estimated Fix Effort |
| -------- | ------------------------------ | --------------------------------- | -------------------- |
| P0       | Synchronous file loading       | 20-100s startup on large projects | Medium               |
| P1       | Non-incremental validation     | Repeated work on each keystroke   | High                 |
| P1       | Transitive fragment resolution | O(n\*d) on each validation        | Medium               |
| P2       | Linear workspace lookup        | Scales poorly with multi-project  | Low                  |
| P2       | Full text sync                 | Bandwidth on large files          | Low                  |
| P3       | Workspace symbol search        | Sequential host iteration         | Low                  |
| P3       | Schema merging                 | Linear with schema file count     | Medium               |

---

## Issue 1: Synchronous File Loading at Initialization

### SME Agents Consulted

- **rust-analyzer Expert**: Query-based, lazy loading patterns
- **LSP Expert**: Initialization responsiveness requirements
- **Rust Expert**: Async I/O patterns

### Context

**Location**: `crates/graphql-lsp/src/server.rs:236-471` (`load_all_project_files`)

The LSP initialization loads all project files synchronously during `initialized`:

```rust
// Current implementation (simplified)
async fn load_all_project_files(...) {
    for pattern in patterns {
        for entry in glob::glob(&pattern) {  // Synchronous glob
            let content = std::fs::read_to_string(&path)?;  // Synchronous I/O
            collected_files.push((file_path, content, file_kind));
        }
    }
    // Then batch-add to host
}
```

**Problem**:

- `glob::glob()` is synchronous filesystem traversal
- `std::fs::read_to_string()` is synchronous I/O
- Blocks the async runtime, preventing LSP from responding
- **Scaling**: ~2ms per file → 20s for 10,000 files, 100s for 50,000 files

**Evidence**:

```rust
// Line 352-367: Warning at 1000 files
if files_scanned == MAX_FILES_WARNING_THRESHOLD {
    tracing::warn!("Loading large number of files ({}+), this may take a while...",
                   MAX_FILES_WARNING_THRESHOLD);
}
```

### Proposed Solution

**Approach**: Progressive/lazy loading with async I/O

```rust
async fn load_all_project_files(...) {
    // Phase 1: Load schema files first (required for validation)
    // Schema files are typically few (<100)
    let schema_files = load_schema_files_async(&schema_patterns).await;
    host.add_schema_files(schema_files);

    // Phase 2: Index document file paths (fast - no content read)
    let document_paths = discover_document_paths_async(&document_patterns).await;
    host.register_document_paths(document_paths);  // Paths only, no content

    // Phase 3: Load document content lazily on first access
    // When did_open is called OR when diagnostics are requested
}
```

**Key Changes**:

1. Replace `glob::glob()` with `ignore` crate (parallel, respects .gitignore)
2. Replace `std::fs::read_to_string()` with `tokio::fs::read_to_string()`
3. Use `futures::stream::iter().buffer_unordered(N)` for parallel loading
4. Lazy content loading - only read files when accessed

### Alternatives Considered

| Alternative                       | Pros                                    | Cons                                        | Decision     |
| --------------------------------- | --------------------------------------- | ------------------------------------------- | ------------ |
| **Lazy loading (proposed)**       | Instant startup, load on demand         | Slight delay on first file access           | **Selected** |
| **Background loading**            | Startup unblocked, eventual consistency | Complex state management, stale diagnostics | Considered   |
| **Memory-mapped files**           | Fast reads, OS caching                  | Platform differences, complexity            | Rejected     |
| **Parallel sync loading (rayon)** | Simple, faster than sequential          | Still blocks, doesn't scale                 | Rejected     |

### Implementation Notes

The rust-analyzer SME recommends following rust-analyzer's VFS (Virtual File System) pattern:

- Maintain an in-memory file index with paths
- Load content on demand
- Use file watchers for updates
- Never block initialization on content loading

---

## Issue 2: Non-Incremental Apollo-Compiler Validation

### SME Agents Consulted

- **rust-analyzer Expert**: Query-based caching patterns
- **Apollo-rs Expert**: apollo-compiler validation internals
- **GraphQL Specification Expert**: Validation rule semantics

### Context

**Location**: `crates/graphql-analysis/src/validation.rs:78-85`

Every validation call re-runs apollo-compiler's full validation:

```rust
let doc = builder.build();
match if errors.is_empty() {
    doc.validate(valid_schema)  // ← NOT cached by Salsa
        .map(|_| ())
        .map_err(|with_errors| with_errors.errors)
} else {
    Err(errors)
}
```

**Problem**:

- `doc.validate()` is an external function, not a Salsa query
- Re-runs all validation rules on every keystroke
- Validation includes expensive type checking, coercion, and recursive checks
- No benefit from Salsa's memoization

**Impact**: Every edit triggers full validation, even if only whitespace changed.

### Proposed Solution

**Approach**: Wrap apollo-compiler validation in a Salsa tracked query

```rust
/// Cached validation result
#[salsa::tracked]
pub fn cached_document_validation(
    db: &dyn GraphQLAnalysisDatabase,
    document_ast_hash: DocumentAstHash,  // Hash of normalized AST
    schema_hash: SchemaHash,              // Hash of schema
    fragment_deps_hash: FragmentDepsHash, // Hash of referenced fragments
) -> Arc<ValidationResult> {
    // Only re-validates when inputs actually change
    let doc = rebuild_document(...);
    let result = doc.validate(schema);
    Arc::new(result)
}
```

**Key Changes**:

1. Create stable AST hashing for documents
2. Track fragment dependency hashes
3. Cache validation results keyed by (doc_hash, schema_hash, deps_hash)

### Alternatives Considered

| Alternative                       | Pros                            | Cons                             | Decision                   |
| --------------------------------- | ------------------------------- | -------------------------------- | -------------------------- |
| **Hash-based caching (proposed)** | Works with external lib         | Hashing overhead                 | **Selected**               |
| **Fork apollo-compiler**          | Full control, Salsa integration | Maintenance burden               | Rejected                   |
| **Incremental validation**        | Only check changed parts        | Requires apollo-compiler changes | Future work                |
| **Debounce validation**           | Reduces frequency               | Delayed feedback                 | Already implemented in LSP |

### Implementation Notes

The apollo-rs SME notes that apollo-compiler's validation is designed to be run as a whole. Incremental validation would require significant changes to apollo-compiler internals. Hash-based caching is the practical approach.

---

## Issue 3: Transitive Fragment Resolution

### SME Agents Consulted

- **rust-analyzer Expert**: Incremental dependency tracking
- **GraphQL Specification Expert**: Fragment semantics
- **Rust Expert**: Collection performance

### Context

**Location**: `crates/graphql-analysis/src/validation.rs:226-262`

Fragment resolution uses BFS traversal through all fragment dependencies:

```rust
fn collect_referenced_fragments_transitive(...) -> HashSet<String> {
    let spreads_index = graphql_hir::fragment_spreads_index(db, project_files);

    let mut all_referenced = collect_referenced_fragments_from_tree(tree);
    let mut to_process: VecDeque<String> = all_referenced.iter().cloned().collect();
    let mut processed = HashSet::new();

    while let Some(fragment_name) = to_process.pop_front() {
        if processed.contains(&fragment_name) { continue; }
        processed.insert(fragment_name.clone());

        // O(1) lookup, but still iterates all spreads
        if let Some(fragment_spreads) = spreads_index.get(&key) {
            for spread_name in fragment_spreads {
                // ... add to queue
            }
        }
    }
    all_referenced
}
```

**Problem**:

- Called on EVERY validation (not cached)
- BFS traversal is O(n \* d) where n = fragments, d = avg depth
- String cloning in the hot path
- `fragment_spreads_index` aggregates all files (could be large)

**Impact**: With 1000 fragments and average depth 5, this is 5000 hash lookups + string ops per validation.

### Proposed Solution

**Approach**: Cache transitive closure per operation/fragment

```rust
/// Cached transitive fragment dependencies for a specific document
#[salsa::tracked]
pub fn transitive_fragment_deps(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Arc<HashSet<Arc<str>>> {
    // Only recomputes when:
    // 1. This file's content changes
    // 2. A referenced fragment's spreads change
    let direct_refs = collect_referenced_fragments_from_tree(...);
    let mut all_refs = direct_refs.clone();

    for frag_name in direct_refs {
        // Per-fragment query - fine-grained dependency
        let transitive = fragment_transitive_deps(db, project_files, frag_name);
        all_refs.extend(transitive.iter().cloned());
    }

    Arc::new(all_refs)
}

/// Cached transitive deps for a single fragment
#[salsa::tracked]
pub fn fragment_transitive_deps(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: ProjectFiles,
    fragment_name: Arc<str>,
) -> Arc<HashSet<Arc<str>>> {
    // Memoized per fragment - rarely changes
}
```

**Key Changes**:

1. Make transitive resolution a Salsa query (cached)
2. Per-fragment transitive closure (fine-grained invalidation)
3. Use `Arc<str>` instead of `String` (zero-copy)
4. Add cycle detection with early termination

### Alternatives Considered

| Alternative                             | Pros                      | Cons                               | Decision     |
| --------------------------------------- | ------------------------- | ---------------------------------- | ------------ |
| **Per-fragment Salsa cache (proposed)** | Fine-grained, incremental | Some overhead                      | **Selected** |
| **Global closure cache**                | Simple                    | Invalidates on any fragment change | Rejected     |
| **Bloom filter pre-check**              | Fast "no fragments" path  | Complexity, false positives        | Considered   |
| **Limit traversal depth**               | Bounds worst case         | May miss deep deps                 | Rejected     |

### Implementation Notes

The rust-analyzer SME emphasizes that making this a Salsa query is critical. The current implementation re-computes on every validation call, losing the benefit of incremental computation that the rest of the architecture provides.

---

## Issue 4: Linear Workspace/Project Lookup

### SME Agents Consulted

- **LSP Expert**: Request handling patterns
- **Rust Expert**: Collection performance

### Context

**Location**: `crates/graphql-lsp/src/server.rs:536-571`

Every LSP request must find the workspace and project for a document:

```rust
fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
    // Fast path: Check if file is already loaded in any host
    for host_entry in self.hosts.iter() {  // O(n) iteration
        let (workspace_uri, project_name) = host_entry.key();
        let host_mutex = host_entry.value();

        if let Ok(host) = host_mutex.try_lock() {  // May fail under contention
            if host.contains_file(&file_path) {
                return Some((workspace_uri.clone(), project_name.clone()));
            }
        }
    }

    // Fallback: Pattern matching (also O(n))
    for workspace_entry in self.workspace_roots.iter() {
        // ...
    }
    None
}
```

**Problem**:

- O(n) iteration over all hosts on EVERY request
- `try_lock()` may skip hosts under contention
- Pattern matching fallback also O(n)
- Called for: `did_change`, `did_open`, hover, completion, goto_definition, etc.

**Impact**: With 10 projects, this is 10 lock attempts per keystroke.

### Proposed Solution

**Approach**: Maintain a reverse index from file URI to (workspace, project)

```rust
struct GraphQLLanguageServer {
    // ... existing fields ...

    /// Reverse index: file URI → (workspace_uri, project_name)
    file_to_project: Arc<DashMap<String, (String, String)>>,
}

fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
    // O(1) lookup
    self.file_to_project
        .get(&document_uri.to_string())
        .map(|entry| entry.value().clone())
}

// Update index when files are added
fn on_file_added(&self, uri: &str, workspace: &str, project: &str) {
    self.file_to_project.insert(uri.to_string(), (workspace.to_string(), project.to_string()));
}
```

**Key Changes**:

1. Add `file_to_project: DashMap<String, (String, String)>` field
2. Populate during `load_all_project_files`
3. Update on `did_open` when file is first seen
4. O(1) lookup instead of O(n) iteration

### Alternatives Considered

| Alternative                      | Pros                   | Cons                        | Decision     |
| -------------------------------- | ---------------------- | --------------------------- | ------------ |
| **Reverse index (proposed)**     | O(1) lookup            | Memory overhead             | **Selected** |
| **Sorted hosts + binary search** | O(log n)               | Complex key design          | Rejected     |
| **Trie-based path lookup**       | Good for path prefixes | Complexity                  | Rejected     |
| **Cache last lookup**            | Fast repeated access   | Doesn't help diverse access | Partial use  |

---

## Issue 5: Full Text Document Sync

### SME Agents Consulted

- **LSP Expert**: Document synchronization modes
- **rust-analyzer Expert**: Incremental text updates

### Context

**Location**: `crates/graphql-lsp/src/server.rs:718-719`

The LSP uses full document sync:

```rust
text_document_sync: Some(TextDocumentSyncCapability::Kind(
    TextDocumentSyncKind::FULL,  // ← Sends entire document on each change
)),
```

**Problem**:

- Every keystroke sends the ENTIRE file content
- Large files (10KB+) = 10KB+ per keystroke
- Network/IPC overhead
- Extra string allocations

**Impact**: Mostly affects large files and remote development scenarios.

### Proposed Solution

**Approach**: Implement incremental sync with text patching

```rust
// Server capabilities
text_document_sync: Some(TextDocumentSyncCapability::Options(
    TextDocumentSyncOptions {
        open_close: Some(true),
        change: Some(TextDocumentSyncKind::INCREMENTAL),
        save: Some(TextDocumentSaveOptions::default()),
        ..Default::default()
    }
)),

// In did_change handler
async fn did_change(&self, params: DidChangeTextDocumentParams) {
    for change in params.content_changes {
        if let Some(range) = change.range {
            // Incremental: apply patch to existing content
            let current_content = self.get_file_content(&uri);
            let new_content = apply_text_edit(current_content, range, &change.text);
            self.update_file(&uri, new_content);
        } else {
            // Full sync fallback
            self.update_file(&uri, change.text);
        }
    }
}
```

**Key Changes**:

1. Advertise `INCREMENTAL` sync capability
2. Maintain file content in server (already done via AnalysisHost)
3. Apply text edits incrementally
4. Handle line index updates

### Alternatives Considered

| Alternative                     | Pros                    | Cons                      | Decision      |
| ------------------------------- | ----------------------- | ------------------------- | ------------- |
| **Incremental sync (proposed)** | Less data transfer      | Implementation complexity | **Selected**  |
| **Keep full sync**              | Simple, already working | Bandwidth on large files  | Current state |
| **Hybrid (detect large files)** | Best of both            | Edge case handling        | Considered    |

### Implementation Notes

The LSP SME notes that incremental sync is the standard for production language servers. The implementation requires careful handling of UTF-16 offsets and line index maintenance.

---

## Issue 6: Workspace Symbol Search

### SME Agents Consulted

- **LSP Expert**: Workspace symbol performance
- **rust-analyzer Expert**: Index-based symbol lookup

### Context

**Location**: `crates/graphql-lsp/src/server.rs:1209-1238`

Workspace symbol search iterates all hosts sequentially:

```rust
async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<...> {
    let mut all_symbols = Vec::new();

    for entry in self.hosts.iter() {
        let host = entry.value();
        let analysis = {
            let host_guard = host.lock().await;  // Sequential locking
            host_guard.snapshot()
        };

        let symbols = analysis.workspace_symbols(&params.query);
        for symbol in symbols {
            all_symbols.push(convert_ide_workspace_symbol(symbol));
        }
    }

    Ok(Some(OneOf::Right(all_symbols)))
}
```

**Problem**:

- Sequential iteration and locking of all hosts
- Each `workspace_symbols` call may scan all files
- No early termination on match limit
- No parallel execution across hosts

### Proposed Solution

**Approach**: Parallel host queries with result limiting

```rust
async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<...> {
    const MAX_RESULTS: usize = 100;

    // Collect snapshots first (minimal lock time)
    let snapshots: Vec<_> = self.hosts.iter()
        .filter_map(|entry| {
            entry.value().try_lock().ok().map(|h| h.snapshot())
        })
        .collect();

    // Query in parallel (snapshots are thread-safe)
    let results: Vec<_> = futures::future::join_all(
        snapshots.iter().map(|analysis| async {
            analysis.workspace_symbols(&params.query)
        })
    ).await;

    // Flatten and limit
    let all_symbols: Vec<_> = results
        .into_iter()
        .flatten()
        .take(MAX_RESULTS)
        .map(convert_ide_workspace_symbol)
        .collect();

    Ok(Some(OneOf::Right(all_symbols)))
}
```

**Key Changes**:

1. Acquire all snapshots first (minimize lock hold time)
2. Query snapshots in parallel (they're thread-safe)
3. Apply result limit to avoid overwhelming the UI
4. Consider pre-built symbol index for faster queries

### Alternatives Considered

| Alternative                     | Pros                      | Cons                      | Decision           |
| ------------------------------- | ------------------------- | ------------------------- | ------------------ |
| **Parallel queries (proposed)** | Faster, scales with cores | Needs snapshot pattern    | **Selected**       |
| **Pre-built index**             | O(1) lookup               | Memory, staleness         | Future enhancement |
| **Limit per host**              | Bounds per-host work      | May miss relevant symbols | Rejected           |

---

## Issue 7: Schema Merging on Large Schemas

### SME Agents Consulted

- **Apollo-rs Expert**: Schema builder performance
- **GraphQL Specification Expert**: Schema composition

### Context

**Location**: `crates/graphql-analysis/src/merged_schema.rs:58-140`

Schema merging iterates all schema files and parses/merges them:

```rust
pub fn merged_schema_with_diagnostics(...) -> MergedSchemaResult {
    let schema_ids = project_files.schema_file_ids(db).ids(db);

    let mut builder = apollo_compiler::schema::SchemaBuilder::new();
    let mut parser = Parser::new();

    for file_id in schema_ids.iter() {
        let (content, metadata) = graphql_db::file_lookup(db, project_files, *file_id)?;
        let text = content.text(db);

        // Parse and merge each file
        parser.parse_into_schema_builder(text.as_ref(), uri.as_str(), &mut builder);
    }

    builder.build()
}
```

**Problem**:

- Re-parses all schema files on each call
- `parse_into_schema_builder` is not cached
- Large schemas (100+ files) = significant parsing time
- Called whenever schema diagnostics are needed

**Impact**: Schema with 100 files, 10KB each = 1MB parsing per validation cycle.

### Proposed Solution

**Approach**: Cache per-file parsed schema ASTs

```rust
/// Cached schema AST for a single file
#[salsa::tracked]
pub fn file_schema_ast(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<apollo_compiler::ast::Document> {
    let text = content.text(db);
    let uri = metadata.uri(db);
    Arc::new(Parser::new().parse_ast(text.as_ref(), uri.as_str()))
}

/// Merged schema using cached per-file ASTs
#[salsa::tracked]
pub fn merged_schema_with_diagnostics(...) -> MergedSchemaResult {
    let mut builder = SchemaBuilder::new();

    for file_id in schema_ids.iter() {
        let (content, metadata) = file_lookup(...)?;
        // Use cached AST
        let ast = file_schema_ast(db, content, metadata);
        builder.add_ast(&ast);  // No re-parsing!
    }

    builder.build()
}
```

**Key Changes**:

1. Add `file_schema_ast` Salsa query for per-file caching
2. Use cached ASTs in `merged_schema_with_diagnostics`
3. Only re-parse files whose content actually changed

### Alternatives Considered

| Alternative                       | Pros                      | Cons                      | Decision           |
| --------------------------------- | ------------------------- | ------------------------- | ------------------ |
| **Per-file AST cache (proposed)** | Incremental, Salsa-native | Needs apollo-compiler API | **Selected**       |
| **Full schema cache**             | Simple                    | Invalidates on any change | Current state      |
| **Persistent cache**              | Survives restarts         | Staleness, complexity     | Future enhancement |

---

## Existing Optimizations (Strengths)

The codebase already implements several excellent optimizations:

### FileEntryMap Pattern

**Location**: `crates/graphql-db/src/lib.rs:98-121`

Per-file granular caching via `FileEntryMap` ensures editing file A doesn't invalidate queries for file B. This is a critical optimization that's already working well.

### Batched File Loading

**Location**: `crates/graphql-lsp/src/server.rs:309-428`

Files are collected without holding locks, then batch-added. This eliminates O(n) lock acquisitions.

### Structure/Body Separation

**Location**: Throughout `graphql-hir`

The separation of structure (type names, signatures) from bodies (selection sets) ensures body edits don't invalidate schema knowledge.

### Per-Fragment Queries

**Location**: `crates/graphql-hir/src/lib.rs:311-328`

`fragment_source` uses per-fragment fine-grained queries instead of loading all fragments.

### Snapshot Pattern

**Location**: `crates/graphql-ide/src/lib.rs`

The `Analysis` snapshot enables lock-free queries after a single lock acquisition.

### Document Version Tracking

**Location**: `crates/graphql-lsp/src/server.rs:908-921`

Version tracking prevents processing stale document updates.

---

## Recommended Priority Order

### P0 - Critical (Do First)

1. **Issue 1: Async File Loading** - Blocks large project adoption

### P1 - High Priority

2. **Issue 3: Cache Transitive Fragment Resolution** - Repeated work on every validation
3. **Issue 2: Cache Apollo Validation** - Repeated work on every keystroke

### P2 - Medium Priority

4. **Issue 4: Reverse Index for Workspace Lookup** - Simple fix, good payoff
5. **Issue 5: Incremental Text Sync** - Standard practice, reduces bandwidth

### P3 - Lower Priority

6. **Issue 6: Parallel Workspace Symbol Search** - Only affects symbol search
7. **Issue 7: Cache Schema ASTs** - Benefits large schemas only

---

## Benchmarks to Add

The following benchmarks should be added to validate fixes:

```rust
// In benches/benches/incremental_computation.rs

/// Benchmark initialization with 1000+ files
fn bench_large_project_init(c: &mut Criterion) { ... }

/// Benchmark transitive fragment resolution with 100+ fragments
fn bench_deep_fragment_chain(c: &mut Criterion) { ... }

/// Benchmark validation caching effectiveness
fn bench_validation_warm_vs_cold(c: &mut Criterion) { ... }

/// Benchmark workspace lookup with 10+ projects
fn bench_workspace_lookup(c: &mut Criterion) { ... }
```

---

## Appendix: SME Agent Summary

| Agent                            | Issues Consulted | Key Insights                                       |
| -------------------------------- | ---------------- | -------------------------------------------------- |
| **rust-analyzer Expert**         | 1, 2, 3, 6       | Lazy loading, Salsa query design, snapshot pattern |
| **LSP Expert**                   | 1, 4, 5, 6       | Responsiveness requirements, incremental sync      |
| **Rust Expert**                  | 1, 3, 4          | Async I/O, collection performance, Arc usage       |
| **Apollo-rs Expert**             | 2, 7             | Validation internals, schema builder APIs          |
| **GraphQL Specification Expert** | 2, 3             | Fragment semantics, validation rules               |

---

**End of Document**

This analysis should be updated as issues are addressed and new bottlenecks are discovered.
