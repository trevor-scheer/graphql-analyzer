# Edit Invalidation Analysis

Analysis of the current edit-to-diagnostic pipeline and how cross-file invalidation works (and doesn't).

## Current Architecture Overview

The system has four layers involved in edit invalidation:

```
LSP (did_change)
  -> IDE (AnalysisHost: update file content in Salsa DB)
    -> Analysis (Salsa-tracked validation/lint queries)
      -> HIR (structure/body split, per-file and aggregate queries)
        -> Syntax (parsing)
          -> Base-DB (Salsa inputs: FileContent, FileMetadata, ProjectFiles)
```

### How Edits Flow Today

1. **User edits a file** -> LSP `did_change` fires
2. **LSP handler** calls `host.add_file_and_snapshot()` to update the Salsa input (`FileContent.text`) for that file
3. **LSP handler** calls `validate_file_with_snapshot()` which calls `snapshot.diagnostics(&file_path)` - **only for the changed file**
4. **Diagnostics are published** to the editor - **only for the changed file**

This means: **when you edit file A, only file A gets re-diagnosed.** Files B, C, D that depend on file A's content are never re-validated until they are themselves edited, reopened, or the user saves (which triggers only lint rules, not validation).

### What Salsa Handles vs. What It Doesn't

Salsa handles memoization and dependency tracking within a single query tree. When file A's content changes, Salsa knows that any cached result depending on A's `FileContent` is stale. The next time someone _asks_ for that result, Salsa will recompute it.

**The key insight: Salsa is pull-based, not push-based.** It doesn't proactively tell you "hey, file B's diagnostics are now stale because file A changed." You have to ask for file B's diagnostics again, and then Salsa will recompute them. The current LSP layer never asks.

---

## The Problem: Missing Cross-File Diagnostic Refresh

### Scenario 1: Rename a Schema Field

```
schema.graphql:  type User { name: String }  ->  type User { displayName: String }
query.graphql:   query { user { name } }     # should now show error, but doesn't
```

**What happens today:**

- `did_change` fires for `schema.graphql`
- LSP publishes diagnostics for `schema.graphql` (no errors)
- `query.graphql` is never re-validated
- User sees no error until they edit `query.graphql` or restart the LSP

**What should happen:**

- `query.graphql` should immediately show `"Cannot query field 'name' on type 'User'"`

### Scenario 2: Delete a Schema Field

Same as above - removing a field from the schema should immediately surface errors on all operations referencing that field.

### Scenario 3: Rename a Fragment

```
fragments.graphql:  fragment UserInfo -> fragment UserDetails
operations.graphql: query { user { ...UserInfo } }  # should show "unknown fragment"
```

**What happens today:**

- `did_change` fires for `fragments.graphql`
- LSP publishes diagnostics for `fragments.graphql` only
- `operations.graphql` still shows no error

### Scenario 4: Delete a Fragment

Same as renaming - the fragment spread becomes invalid but no error surfaces.

### Scenario 5: Change a Fragment's Type Condition

```
fragments.graphql:  fragment UserInfo on User -> fragment UserInfo on Post
operations.graphql: query { user { ...UserInfo } }  # type mismatch, but no error shown
```

### Scenario 6: Add a Required Field/Argument to Schema

```
schema.graphql: type Query { user(id: ID!): User }  # id is now required
query.graphql:  query { user { name } }              # missing required arg, no error shown
```

---

## Analysis of the Dependency Graph

Here's what depends on what, and therefore what needs re-validation when something changes:

### Schema File Changes

When a schema file's content changes, these Salsa queries become stale:

| Query                              | What It Computes             | Who Depends On It                                        |
| ---------------------------------- | ---------------------------- | -------------------------------------------------------- |
| `file_type_defs(schema_file)`      | Types from one schema file   | `schema_types()`                                         |
| `schema_types(project_files)`      | All merged types             | Every document validation, every completion, every hover |
| `merged_schema_with_diagnostics()` | The apollo-compiler `Schema` | `validate_file()` for every document file                |

**Impact**: A schema edit invalidates the validation result of **every document file** in the project.

### Document File Changes (Operations/Fragments)

When a document file's content changes:

| Query                            | What It Computes          | Who Depends On It                                    |
| -------------------------------- | ------------------------- | ---------------------------------------------------- |
| `file_fragments(doc_file)`       | Fragments from one file   | `all_fragments()`, `fragment_file_index()`, etc.     |
| `file_operations(doc_file)`      | Operations from one file  | `all_operations()`, name indexes                     |
| `all_fragments(project_files)`   | All fragments             | `validate_file()` for any file using fragments       |
| `fragment_spreads_index()`       | Fragment dependency graph | `validate_file()` for any file with fragment spreads |
| `project_fragment_name_index()`  | Fragment name uniqueness  | Fragment uniqueness checks in other files            |
| `project_operation_name_index()` | Operation name uniqueness | Operation uniqueness checks in other files           |

**Impact**: A document edit can invalidate:

- Validation of any file that uses fragments defined in this file
- Uniqueness checks in files with same-named operations/fragments
- Fragment transitive dependency resolution

---

## Exact Inputs for a Single File's Diagnostics

Before deciding on a refresh strategy, we need to know: **does diagnosing file F require reading all 10k files, or just a narrow subset?**

The answer: **a narrow subset.** Here's the exact dependency tree for `file_diagnostics(file_F)`:

### `validate_file()` (validation.rs) depends on:

| Input                                | Scope                                                | Notes                                                                                       |
| ------------------------------------ | ---------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `merged_schema_with_diagnostics(pf)` | All schema files (typically 1-5)                     | Single cached `Schema` object, shared by all document files                                 |
| `fragment_spreads_index(pf)`         | All document files' fragment spreads                 | Aggregate index, but per-file contributions are cached independently                        |
| `fragment_ast(pf, "FragName")`       | **Only the specific fragments this file references** | Fine-grained: creates Salsa dependency on just the referenced fragment files, not all files |

### `lint_file_impl()` (lint_integration.rs) depends on:

| Input                              | Scope                               | Notes                                                 |
| ---------------------------------- | ----------------------------------- | ----------------------------------------------------- |
| `schema_types(pf)`                 | All schema files                    | Same shared schema                                    |
| `project_fragment_name_index(pf)`  | All document files' fragment names  | Only queried if file has fragments (uniqueness check) |
| `project_operation_name_index(pf)` | All document files' operation names | Only queried if file has named operations             |

### How Aggregate Indexes Work at 10k Files

The aggregate indexes (`fragment_spreads_index`, `project_fragment_name_index`, etc.) do iterate all files internally. But Salsa's per-file granularity means:

1. If only file A changed, only `file_fragment_spreads(A)` recomputes
2. The aggregate merges 1 fresh result + 9,999 cached results
3. If the aggregate output is unchanged (e.g., editing a selection set didn't add/remove fragment spreads), **Salsa backdates the result** and nothing downstream re-runs

The actual recomputation cost is proportional to the number of files that changed, not the total number of files.

### Bottom Line

A single file's diagnostics depend on:

- The merged schema (shared, cached)
- Specific fragments it references by name (fine-grained)
- Aggregate name indexes (cheap to verify, backdate when unchanged)

It does **not** depend on the content of all 10k files. This makes selective refresh viable.

---

## What Needs to Change: The "Affected Files" Problem

The core question is: **after editing file A, which other files need their diagnostics refreshed?**

### Do We Need to Handle Every Edit Type Individually?

**No.** We don't need to classify individual edits (rename vs delete vs add). But we do need to classify the **kind of file** that changed (schema vs document) and **what aspect** changed (structure vs body). This gives us a small number of categories with clear refresh rules.

### Strategy: Debounced Selective Refresh (Recommended)

Combine immediate per-file validation with debounced cross-file refresh, scoped to affected files:

```
on did_change(file_A):
    // Immediate: validate the changed file
    publish_diagnostics(file_A)

    // Debounced (300ms): refresh only affected files
    debounce("cross-file-refresh", 300ms, || {
        affected = compute_affected_files(file_A)
        for F in affected:
            publish_diagnostics(F)
    })
```

**Why debounce:** Coalesces rapid keystrokes. While a user types `displ` -> `display` -> `displayName`, only the final state triggers cross-file work.

**Why selective:** At 10k files, we can't re-publish all files every 300ms. But we can refresh the ~5-50 files that actually depend on what changed.

---

## Detailed Design

### What "Affected Files" Means Per Edit Category

#### Category 1: Schema Content Changes

**Trigger:** Any edit to a file with `DocumentKind::Schema`
**Affected:** ALL document files (every operation/fragment validates against the schema via `merged_schema_with_diagnostics()`)
**Mitigation:** Salsa's backdate optimization. If the schema edit doesn't change the merged `Schema` output (e.g., editing a description or comment), the backdate means zero downstream recomputation. In practice: requery all document files, but Salsa makes this ~free when the schema semantics didn't change.

**At 10k files:** Worst case (actual schema change like field rename) requires re-publishing 10k files. But Salsa only actually recomputes validation for files that query the changed field. Others get cache hits. The cost is O(10k) Salsa verification checks (fast) + O(affected) actual recomputation.

#### Category 2: Fragment Structure Changes (Name, Type Condition)

**Trigger:** Renaming a fragment, changing its type condition, adding/removing a fragment
**Affected:** Files that spread the changed fragment (directly or transitively)
**How to find them:** Build a reverse index from `fragment_spreads_index()`:

```
fragment_spreads_index: { "FragA" -> {"FragB", "FragC"}, "FragB" -> {} }
reverse: { "FragB" -> {"FragA"}, "FragC" -> {"FragA"} }
```

Then walk the reverse graph + find files that contain operations spreading these fragments.

**At 10k files:** Typically affects 1-20 files. Very targeted.

#### Category 3: Fragment Body Changes (Selection Set)

**Trigger:** Editing fields inside a fragment body (not changing name or type condition)
**Affected:** Files that spread this fragment - because `validate_file()` pulls the fragment's AST via `fragment_ast()`, and the AST changed
**At 10k files:** Same as Category 2, typically 1-20 files.

**Optimization opportunity:** Fragment body edits are the trickiest because they cross the structure/body boundary via `fragment_ast()`. If we could detect "fragment name unchanged, type condition unchanged, only selection set changed" at the LSP layer, we'd still need to refresh spreaders (for validation) but could skip name index verification.

#### Category 4: Operation Structure Changes (Name, Variables)

**Trigger:** Renaming an operation, changing variables
**Affected:** Other files with same-named operations (uniqueness check via `project_operation_name_index()`)
**At 10k files:** Usually 0-2 files (name collisions are rare). Could be zero if the old or new name is unique.

#### Category 5: Operation Body Changes

**Trigger:** Editing fields inside an operation body
**Affected:** **Only the file itself.** Operations don't have cross-file dependents.
**At 10k files:** Zero cross-file refresh needed. This is the most common edit.

### Implementation Sketch

```rust
// In the LSP did_change handler:
async fn did_change(&self, params: DidChangeTextDocumentParams) {
    // ... existing file update logic ...

    // Immediate: validate changed file
    self.validate_file_with_snapshot(&uri, snapshot.clone()).await;

    // Schedule debounced cross-file refresh
    self.schedule_cross_file_refresh(
        workspace_uri, project_name, uri, document_kind
    ).await;
}

fn compute_affected_files(
    &self,
    snapshot: &Analysis,
    changed_uri: &Uri,
    document_kind: DocumentKind,
) -> Vec<FilePath> {
    if document_kind == DocumentKind::Schema {
        // Schema change: all document files need refresh
        return snapshot.all_document_file_paths();
    }

    // Document file change: check what changed
    let changed_path = FilePath::new(changed_uri.to_string());
    let mut affected = Vec::new();

    // Check if fragment names/operations changed using structure comparison
    // (This can be optimized with fingerprinting later)
    let fragments_in_file = snapshot.fragments_in_file(&changed_path);
    if !fragments_in_file.is_empty() {
        // Files that spread any fragment defined in the changed file
        let spreaders = snapshot.files_spreading_fragments(&fragments_in_file);
        affected.extend(spreaders);
    }

    let operations_in_file = snapshot.operations_in_file(&changed_path);
    if !operations_in_file.is_empty() {
        // Files with same-named operations (uniqueness checks)
        let colliders = snapshot.files_with_operation_names(&operations_in_file);
        affected.extend(colliders);
    }

    affected
}
```

### Salsa's "Backdate" Optimization

Salsa has a critical optimization: when a tracked function's inputs change but the output doesn't, Salsa "backdates" the output (marks it as unchanged). Downstream queries don't need to recompute.

Example:

- Edit `schema.graphql` to add a comment
- `file_type_defs(schema_file)` recomputes, but returns the **same** `TypeDef` map
- Salsa backdates -> `schema_types()` doesn't recompute
- `validate_file()` for all document files returns cached results

This means even the "refresh all document files on schema change" path is cheap when the schema semantics didn't actually change. The cost is just Salsa verifying the dependency chain (HashMap equality checks), not rerunning validation.

---

## Current Structure/Body Split Assessment

The existing split is well-designed at the conceptual level:

| Layer                            | What It Captures         | When It Invalidates            |
| -------------------------------- | ------------------------ | ------------------------------ |
| `FileStructureData`              | Names, types, signatures | Name/type changes only         |
| `OperationBody` / `FragmentBody` | Selection sets, spreads  | Body content changes           |
| `file_type_defs()`               | Schema types per file    | Schema file content changes    |
| `schema_types()`                 | Merged schema types      | Any schema file content change |

**The split works correctly at the Salsa level.** The primary problem is in the LSP layer not asking Salsa about other files after an edit. However, there are a few known leaks worth documenting:

### Known Leaks in the Structure/Body Split

**1. `file_fragment_spreads()` bridges structure and body** (`hir/src/lib.rs:493`)

This query calls `file_fragments()` (structure) and then `fragment_body()` (body) for each fragment. This means `fragment_spreads_index()` - which aggregates it - is invalidated whenever ANY fragment body changes, not just when names change. The validation layer uses `fragment_spreads_index()` for cross-file resolution, so editing a fragment's selection set cascades to re-validation of files using fragments from that file.

**2. `TextRange` fields in structure types cause false invalidation**

`OperationStructure` and `FragmentStructure` include byte-offset ranges (`name_range`, `operation_range`, `fragment_range`). If editing body content shifts byte offsets (e.g., adding text before a fragment definition in the same file), these ranges change, causing `file_structure()` to produce a different output even though the semantic structure (names, types) hasn't changed. In practice this is mostly contained because operations/fragments are top-level definitions and embedded GraphQL blocks are parsed independently.

**3. `analyze_field_usage()` directly walks bodies**

The `analyze_field_usage()` and `field_usage_for_type()` functions in `project_lints.rs` directly call `operation_body()` and `fragment_body()` for all operations. When any operation file changes, these must re-fetch the changed operation's body. The `find_unused_fields()` function was fixed to use per-file aggregation (`all_used_schema_coordinates()`), but the detailed analysis functions still cross the boundary.

None of these leaks are bugs - they're known trade-offs. The architecture is sound for its primary goal: **preventing schema knowledge recomputation when users edit operation bodies** (the "golden invariant", which is verified by tests).

---

## Recommendations

### Phase 1: Debounced Selective Refresh

1. **Add debounced cross-file diagnostic refresh after `did_change`.**
   - Immediate: publish diagnostics for the changed file (existing behavior)
   - Debounced (300ms): refresh affected files based on edit category
   - Schema change -> all document files
   - Document change -> files that spread its fragments + files with same-named operations

2. **Add `compute_affected_files()` to the IDE layer.**
   - New API: given a changed file, return the set of files that need re-diagnosis
   - Uses existing Salsa queries (`fragment_spreads_index`, name indexes) to determine scope
   - Returns early for operation body-only changes (no cross-file dependents)

3. **Move project-wide lints from `did_save` to the debounced refresh.**
   - Currently project-wide lints (unused fragments, unused fields) only run on save
   - They should be included in the debounced cross-file refresh for consistency

### Phase 2: Optimization

4. **Add structure fingerprinting.**
   - Hash of (fragment names, type conditions, operation names, variable signatures) per file
   - Compare before/after to detect body-only edits at the LSP layer
   - Skip cross-file refresh entirely for body-only edits in document files
   - This handles the most common edit (typing inside an operation) with zero cross-file cost

5. **Build a reverse fragment dependency index.**
   - `fragment_name -> Vec<FileId>` mapping files that spread each fragment
   - Salsa-tracked, per-file granularity (like existing indexes)
   - Enables O(affected) instead of O(all_documents) lookup for fragment changes

---

## Summary

| Question                                               | Answer                                                                                                                                                                           |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Do we need to handle every edit type?                  | **No.** Classify by file kind (schema/document) and change kind (structure/body). Five categories cover everything.                                                              |
| What's missing?                                        | The LSP layer only re-validates the edited file. Affected files are never refreshed.                                                                                             |
| What's the fix?                                        | Debounced selective cross-file refresh after each edit.                                                                                                                          |
| Is it expensive at 10k files?                          | No. Operation body edits (most common): zero cross-file work. Fragment changes: ~1-20 files. Schema changes: Salsa verification is O(n) but actual recomputation is O(affected). |
| Does a file's diagnostics depend on all project files? | **No.** Depends on merged schema (shared/cached) + specific referenced fragments (fine-grained) + aggregate indexes (cheap to verify).                                           |
| Is the Salsa architecture correct?                     | Yes. The gap is only in the LSP orchestration layer not asking Salsa about other files.                                                                                          |
