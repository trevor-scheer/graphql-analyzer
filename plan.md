# Plan: Local vs Project-Wide Validation Scoping

## Goal

Limit `didChange` (every keystroke) to **local-only** validations and lints for faster feedback. Run full **project-wide** validation and lints only on `didSave`.

## Current State

**didChange** calls `Analysis::diagnostics()` -> `file_diagnostics()` which runs:
- Syntax validation (parse errors)
- Apollo-compiler validation (needs schema + cross-file fragments)
- All per-file lints (standalone document + document-schema)

**didSave** calls `Analysis::all_diagnostics_for_change()` which runs:
- Everything from `diagnostics()` for the changed file + affected files
- Project-wide lint rules (unused_fragments, unused_fields, unique_names)

## Problem

`didChange` does too much work - it runs schema validation and cross-file fragment resolution on every keystroke. This is expensive for large projects.

## Rule Classification

### Local (safe for didChange - no cross-file data needed)
| Rule | Type | Rationale |
|------|------|-----------|
| Syntax errors | Validation | Only needs current file's text |
| `no_anonymous_operations` | StandaloneDocumentLintRule | `_project_files` unused |
| `operation_name_suffix` | StandaloneDocumentLintRule | `_project_files` unused |
| `unused_variables` | StandaloneDocumentLintRule | `_project_files` unused |

### Project-wide (didSave only - needs cross-file data)
| Rule | Type | Rationale |
|------|------|-----------|
| Apollo-compiler validation | Validation | Needs schema + cross-file fragment resolution |
| `redundant_fields` | StandaloneDocumentLintRule | Calls `all_fragments()` - actually project-wide |
| `no_deprecated` | DocumentSchemaLintRule | Needs `schema_types()` |
| `require_id_field` | DocumentSchemaLintRule | Needs `schema_types()` |
| `unique_names` | ProjectLintRule | Needs all operations across project |
| `unused_fragments` | ProjectLintRule | Needs all operations across project |
| `unused_fields` | ProjectLintRule | Needs all operations + schema |

## Implementation Steps

### Step 1: Add `ValidationScope` to lint rule traits

**File: `crates/linter/src/traits.rs`**

Add a `ValidationScope` enum and a `scope()` method to the `LintRule` base trait:

```rust
/// Whether a rule can run with only local file data or needs project-wide context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationScope {
    /// Only needs the current file's parsed content. Safe for didChange.
    Local,
    /// Needs cross-file data (schema, fragments from other files). Runs on didSave.
    Project,
}
```

Add to `LintRule` trait:
```rust
fn scope(&self) -> ValidationScope;
```

`ProjectLintRule` impls always return `Project`. Individual `StandaloneDocumentLintRule` and `DocumentSchemaLintRule` impls return their actual scope.

### Step 2: Classify each lint rule

**Files: `crates/linter/src/rules/*.rs`**

Implement `scope()` on each rule:
- `no_anonymous_operations` -> `Local`
- `operation_name_suffix` -> `Local`
- `unused_variables` -> `Local`
- `redundant_fields` -> `Project` (uses `all_fragments()`)
- `no_deprecated` -> `Project` (uses `schema_types()`)
- `require_id_field` -> `Project` (uses `schema_types()`)
- `unique_names` -> `Project` (inherent)
- `unused_fragments` -> `Project` (inherent)
- `unused_fields` -> `Project` (inherent)

### Step 3: Add scope-filtered lint execution in analysis

**File: `crates/analysis/src/lint_integration.rs`**

Add a new tracked function `lint_file_local_impl()` that only runs rules where `scope() == Local`:

```rust
/// Run only local lints (no cross-file data access). For didChange.
#[salsa::tracked]
fn lint_file_local_impl(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Arc<Vec<Diagnostic>>
```

This filters `standalone_document_rules()` to only those with `scope() == Local`, skips `document_schema_rules()` entirely (all are currently `Project`), and skips schema lints.

Add a public wrapper:
```rust
pub fn lint_file_local(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: Option<ProjectFiles>,
) -> Arc<Vec<Diagnostic>>
```

### Step 4: Add local-only diagnostics function in analysis

**File: `crates/analysis/src/lib.rs`**

Add new public + tracked functions:

```rust
/// Get local-only diagnostics (syntax errors + local lints). For didChange.
pub fn file_local_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: Option<ProjectFiles>,
) -> Arc<Vec<Diagnostic>>

#[salsa::tracked]
fn file_local_diagnostics_impl(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Arc<Vec<Diagnostic>>
```

This returns syntax errors + local lints only. No apollo-compiler validation.

### Step 5: Add local diagnostics method to IDE Analysis

**File: `crates/ide/src/analysis.rs`**

Add a new method:

```rust
/// Get local-only diagnostics for a file (syntax + local lints). For didChange.
pub fn local_diagnostics(&self, file: &FilePath) -> Vec<Diagnostic>
```

This calls `file_local_diagnostics()` instead of `file_diagnostics()`.

### Step 6: Update LSP didChange handler

**File: `crates/lsp/src/server.rs`**

Change `validate_file_with_snapshot()` (called from `didChange`) to use `snapshot.local_diagnostics()` instead of `snapshot.diagnostics()`.

### Step 7: Ensure didSave publishes complete diagnostics

**File: `crates/lsp/src/server.rs`**

The current `didSave` handler already calls `all_diagnostics_for_change()` which runs full validation + all lints + project lints. This is correct and needs no change.

`all_diagnostics_for_change()` uses full `diagnostics()` (not `local_diagnostics()`) so project-wide validation results replace the local-only results from didChange. The LSP protocol's `publishDiagnostics` replaces the full diagnostic set for a URI.

### Step 8: Document the design decision

**File: `docs/design/local-vs-project-validation.md`**

Create an internal design document covering:
- Motivation: performance on didChange
- Classification criteria (local vs project)
- Rule classification table
- How to classify new rules (guide for contributors)
- The `ValidationScope` enum and `scope()` method
- How didChange and didSave interact (local results get replaced by full results on save)

### Step 9: Update Claude context docs

**File: `crates/linter/CLAUDE.md`**

Add guidance about `ValidationScope` and how new rules should declare their scope.

**File: `crates/CLAUDE.md`**

Update the "Protected Core Features" table to note the local/project split for real-time diagnostics.

### Step 10: Tests

- Unit test in `crates/linter/` verifying each rule's `scope()` matches expectations
- Integration test in `crates/analysis/` verifying `file_local_diagnostics()` only returns syntax + local lint diagnostics
- Integration test verifying `file_diagnostics()` still returns everything (backward compat for didSave path)

## Key Design Decisions

1. **Scope lives on the rule, not the trait** - A `StandaloneDocumentLintRule` can be either local or project. This avoids splitting the trait hierarchy and is forward-compatible with future rules.

2. **`redundant_fields` is project-scoped** despite being a `StandaloneDocumentLintRule` - It calls `all_fragments()` which is project-wide data. The trait name "standalone" means "no schema required", not "no cross-file data".

3. **Validation (apollo-compiler) is project-scoped** - It resolves fragments transitively across files and validates against the merged schema. Only syntax errors are local.

4. **didSave publishes complete diagnostics** - The full set from didSave replaces the partial local set from didChange for the same file. The LSP client receives the latest published diagnostics per URI.

5. **Separate Salsa tracked function for local lints** - `lint_file_local_impl` is separate from `lint_file_impl` to preserve Salsa memoization for each path independently.

## Files Changed (Summary)

| File | Change |
|------|--------|
| `crates/linter/src/traits.rs` | Add `ValidationScope` enum, `scope()` to `LintRule` |
| `crates/linter/src/rules/*.rs` | Implement `scope()` on each rule |
| `crates/analysis/src/lint_integration.rs` | Add `lint_file_local()` / `lint_file_local_impl()` |
| `crates/analysis/src/lib.rs` | Add `file_local_diagnostics()` / `file_local_diagnostics_impl()` |
| `crates/ide/src/analysis.rs` | Add `local_diagnostics()` method |
| `crates/lsp/src/server.rs` | didChange uses `local_diagnostics()` |
| `docs/design/local-vs-project-validation.md` | Design decision doc |
| `crates/linter/CLAUDE.md` | Update contributor guidance |
| `crates/CLAUDE.md` | Update architecture notes |
