# Plan: Local vs Project-Wide Validation Scoping

## Goal

Scope what runs on `didChange` vs `didSave` based on **file type** (document vs schema) to optimize keystroke-level feedback while deferring expensive project-wide work to save.

## Current State

**didChange** calls `Analysis::diagnostics()` -> `file_diagnostics()` which runs:
- Syntax validation (parse errors)
- Apollo-compiler validation (needs schema + cross-file fragments)
- All per-file lints (standalone document + document-schema)

**didSave** calls `Analysis::all_diagnostics_for_change()` which runs:
- Everything from `diagnostics()` for the changed file + affected files
- Project-wide lint rules (unused_fragments, unused_fields, unique_names)

Both paths run identically regardless of whether the changed file is a document or schema.

## Problem

The cost profile differs significantly by file type:
- **Document change**: Validation against the already-merged schema is fast (Salsa-cached). But project-wide lints are unnecessary on every keystroke.
- **Schema change**: Re-merging the schema is expensive and cascades validation to all document files. This should not happen on every keystroke.

## Desired Behavior

### Document file didChange
| What runs | Why |
|-----------|-----|
| Syntax validation | Fast, local |
| Semantic validation (apollo-compiler) | Fast - merged schema is already cached |
| Local lints | Fast, single-file only |

### Document file didSave
| What runs | Why |
|-----------|-----|
| Everything from didChange | Baseline |
| Project-wide lints | Cross-file analysis (unused fragments, unused fields, unique names) |

### Schema file didChange
| What runs | Why |
|-----------|-----|
| Syntax validation | Fast, local |
| Local lints | Fast, single-file only |

### Schema file didSave
| What runs | Why |
|-----------|-----|
| Everything from didChange | Baseline |
| Schema merging + merged schema diagnostics | Expensive, deferred to save |
| Re-validation of all document files | Schema change cascades |
| Project-wide lints | Cross-file analysis |

## Lint Rule Classification

### Local (runs on didChange for both document and schema files)
| Rule | Type | Rationale |
|------|------|-----------|
| `no_anonymous_operations` | StandaloneDocumentLintRule | `_project_files` unused |
| `operation_name_suffix` | StandaloneDocumentLintRule | `_project_files` unused |
| `unused_variables` | StandaloneDocumentLintRule | `_project_files` unused |

### Project-wide (didSave only)
| Rule | Type | Rationale |
|------|------|-----------|
| `redundant_fields` | StandaloneDocumentLintRule | Calls `all_fragments()` - actually project-wide |
| `no_deprecated` | DocumentSchemaLintRule | Needs `schema_types()` |
| `require_id_field` | DocumentSchemaLintRule | Needs `schema_types()` |
| `unique_names` | ProjectLintRule | Needs all operations across project |
| `unused_fragments` | ProjectLintRule | Needs all operations across project |
| `unused_fields` | ProjectLintRule | Needs all operations + schema |

Note: For **document files**, `no_deprecated` and `require_id_field` could also run on didChange since schema is cached. But classifying them as project-wide keeps the model simple and consistent - the schema could become stale during editing. We can revisit if needed.

## Implementation Steps

### Step 1: Add `ValidationScope` to lint rule traits

**File: `crates/linter/src/traits.rs`**

```rust
/// Whether a rule can run with only local file data or needs project-wide context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationScope {
    /// Only needs the current file's parsed content. Runs on didChange.
    Local,
    /// Needs cross-file data (schema, fragments from other files). Runs on didSave.
    Project,
}
```

Add `scope()` method to the `LintRule` base trait:
```rust
fn scope(&self) -> ValidationScope;
```

### Step 2: Classify each lint rule

**Files: `crates/linter/src/rules/*.rs`**

Implement `scope()` on each rule:
- `no_anonymous_operations` -> `Local`
- `operation_name_suffix` -> `Local`
- `unused_variables` -> `Local`
- `redundant_fields` -> `Project` (uses `all_fragments()`)
- `no_deprecated` -> `Project` (uses `schema_types()`)
- `require_id_field` -> `Project` (uses `schema_types()`)
- `unique_names` -> `Project` (inherent to ProjectLintRule)
- `unused_fragments` -> `Project` (inherent)
- `unused_fields` -> `Project` (inherent)

### Step 3: Add scope-filtered lint execution in analysis

**File: `crates/analysis/src/lint_integration.rs`**

Add a new tracked function that only runs local-scoped lints:

```rust
/// Run only local lints (no cross-file data access). For didChange.
pub fn lint_file_local(db, content, metadata, project_files: Option<ProjectFiles>) -> Arc<Vec<Diagnostic>>

#[salsa::tracked]
fn lint_file_local_impl(db, content, metadata, project_files: ProjectFiles) -> Arc<Vec<Diagnostic>>
```

Filters `standalone_document_rules()` to `scope() == Local`. Skips `document_schema_rules()` entirely (all currently `Project`).

### Step 4: Add document-change and schema-change diagnostic functions

**File: `crates/analysis/src/lib.rs`**

Two new public APIs reflecting the two didChange branches:

```rust
/// Diagnostics for document file didChange: syntax + semantic validation + local lints.
pub fn document_change_diagnostics(db, content, metadata, project_files: Option<ProjectFiles>) -> Arc<Vec<Diagnostic>>

#[salsa::tracked]
fn document_change_diagnostics_impl(db, content, metadata, project_files: ProjectFiles) -> Arc<Vec<Diagnostic>>
// Returns: syntax_errors + validate_file() + lint_file_local()
```

```rust
/// Diagnostics for schema file didChange: syntax + local lints only.
/// Schema merging and downstream validation deferred to didSave.
pub fn schema_change_diagnostics(db, content, metadata, project_files: Option<ProjectFiles>) -> Arc<Vec<Diagnostic>>

#[salsa::tracked]
fn schema_change_diagnostics_impl(db, content, metadata, project_files: ProjectFiles) -> Arc<Vec<Diagnostic>>
// Returns: syntax_errors + lint_file_local()
// NO merged_schema_diagnostics_for_file - that's expensive and deferred to save
```

### Step 5: Add change-scoped diagnostics to IDE Analysis

**File: `crates/ide/src/analysis.rs`**

Add a new method that dispatches based on document kind:

```rust
/// Diagnostics appropriate for a didChange event.
/// Documents: syntax + semantic validation + local lints.
/// Schemas: syntax + local lints only (merging deferred to save).
pub fn change_diagnostics(&self, file: &FilePath) -> Vec<Diagnostic>
```

Internally checks `metadata.is_schema()` vs `metadata.is_document()` to call the right analysis function.

### Step 6: Update LSP didChange handler

**File: `crates/lsp/src/server.rs`**

Change `validate_file_with_snapshot()` to use `snapshot.change_diagnostics()` instead of `snapshot.diagnostics()`.

No changes needed to `didSave` - it already calls `all_diagnostics_for_change()` which:
- Runs full `diagnostics()` (syntax + validation + all lints) for affected files
- Merges project-wide lint results
- For schema changes: re-validates all document files

### Step 7: Document the design decision

**File: `docs/design/local-vs-project-validation.md`**

Internal design document covering:
- Motivation: different cost profiles for document vs schema changes
- The two-axis model (file type x event type)
- Rule classification table with rationale
- How to classify new rules
- How didChange and didSave interact (save replaces change diagnostics)

### Step 8: Update Claude context docs

**File: `crates/linter/CLAUDE.md`** - Add `ValidationScope` guidance for new rules.

**File: `crates/CLAUDE.md`** - Update "Protected Core Features" to note the document/schema split.

### Step 9: Tests

- Unit tests in `crates/linter/` verifying each rule's `scope()` value
- Integration test: `document_change_diagnostics()` returns syntax + validation + local lints (no project lints)
- Integration test: `schema_change_diagnostics()` returns syntax + local lints only (no validation, no schema merging)
- Integration test: `file_diagnostics()` still returns everything (backward compat for didSave path)

## Key Design Decisions

1. **Two-axis model**: The scoping depends on both the file type (document vs schema) and the event type (change vs save). This captures the real cost structure.

2. **Document didChange includes semantic validation**: Validating a document against the merged schema is fast because the schema is already Salsa-cached. This gives users immediate field/type error feedback while typing.

3. **Schema didChange excludes merging**: Schema merging is expensive and cascades to all documents. Deferring it to save avoids keystroke-level recomputation of the entire project.

4. **Lint scope lives on the rule, not the trait**: A `StandaloneDocumentLintRule` can be either local or project. This avoids splitting the trait hierarchy.

5. **`redundant_fields` is project-scoped** despite being a `StandaloneDocumentLintRule`: It calls `all_fragments()` which is project-wide data.

6. **didSave replaces didChange diagnostics**: The LSP `publishDiagnostics` replaces the full set for a URI. Save results are a superset of change results.

## Files Changed (Summary)

| File | Change |
|------|--------|
| `crates/linter/src/traits.rs` | Add `ValidationScope` enum, `scope()` to `LintRule` |
| `crates/linter/src/rules/*.rs` | Implement `scope()` on each rule |
| `crates/analysis/src/lint_integration.rs` | Add `lint_file_local()` |
| `crates/analysis/src/lib.rs` | Add `document_change_diagnostics()`, `schema_change_diagnostics()` |
| `crates/ide/src/analysis.rs` | Add `change_diagnostics()` method |
| `crates/lsp/src/server.rs` | didChange uses `change_diagnostics()` |
| `docs/design/local-vs-project-validation.md` | Design decision doc |
| `crates/linter/CLAUDE.md` | Update contributor guidance |
| `crates/CLAUDE.md` | Update architecture notes |
