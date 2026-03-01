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

Salsa handles memoization and dependency tracking within a single query tree. When file A's content changes, Salsa knows that any cached result depending on A's `FileContent` is stale. The next time someone *asks* for that result, Salsa will recompute it.

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

| Query | What It Computes | Who Depends On It |
|-------|-----------------|-------------------|
| `file_type_defs(schema_file)` | Types from one schema file | `schema_types()` |
| `schema_types(project_files)` | All merged types | Every document validation, every completion, every hover |
| `merged_schema_with_diagnostics()` | The apollo-compiler `Schema` | `validate_file()` for every document file |

**Impact**: A schema edit invalidates the validation result of **every document file** in the project.

### Document File Changes (Operations/Fragments)

When a document file's content changes:

| Query | What It Computes | Who Depends On It |
|-------|-----------------|-------------------|
| `file_fragments(doc_file)` | Fragments from one file | `all_fragments()`, `fragment_file_index()`, etc. |
| `file_operations(doc_file)` | Operations from one file | `all_operations()`, name indexes |
| `all_fragments(project_files)` | All fragments | `validate_file()` for any file using fragments |
| `fragment_spreads_index()` | Fragment dependency graph | `validate_file()` for any file with fragment spreads |
| `project_fragment_name_index()` | Fragment name uniqueness | Fragment uniqueness checks in other files |
| `project_operation_name_index()` | Operation name uniqueness | Operation uniqueness checks in other files |

**Impact**: A document edit can invalidate:
- Validation of any file that uses fragments defined in this file
- Uniqueness checks in files with same-named operations/fragments
- Fragment transitive dependency resolution

---

## What Needs to Change: The "Affected Files" Problem

The core question is: **after editing file A, which other files need their diagnostics refreshed?**

### Do We Need to Handle Every Edit Type Individually?

**No.** We don't need to classify edits by type (rename, delete, add field, etc.). Instead, we can use a simpler and more robust approach based on Salsa's own dependency tracking.

There are two viable strategies:

### Strategy 1: Conservative Broadcast (Simple, Correct, Slightly Wasteful)

After any file edit, re-publish diagnostics for all open files (or all loaded files).

```
on did_change(file_A):
    update file_A content in Salsa
    for each loaded file F:
        diagnostics = salsa_query(diagnostics_for(F))  // Salsa handles memoization
        publish_diagnostics(F, diagnostics)
```

**Why this works well enough:** Salsa's memoization means that if file F's diagnostics haven't actually changed (because file A's edit didn't affect F), the query returns the cached result almost instantly. The cost is just the HashMap lookups through the dependency chain.

**Trade-offs:**
- Simple to implement
- Correct for all edit types without special-casing
- Cost per keystroke = O(open_files * Salsa_verification_cost)
- For large projects (hundreds of files), this could add latency

### Strategy 2: Dependency-Aware Selective Refresh (Optimal, More Complex)

Track which files depend on which, and only refresh affected files.

```
on did_change(file_A):
    update file_A content in Salsa
    publish_diagnostics(file_A)

    if file_A is a schema file:
        // Schema change invalidates ALL document files
        for each document_file F:
            publish_diagnostics(F)

    else if file_A is a document file:
        // Check what changed at the structure level
        old_structure = cached file_structure(file_A)
        new_structure = file_structure(file_A)  // recomputed by Salsa

        if structure_changed(old_structure, new_structure):
            // Fragment/operation names changed -> refresh files that reference them
            for each file F that references changed fragments/operations:
                publish_diagnostics(F)
```

**Trade-offs:**
- Minimal work per keystroke
- Requires tracking "reverse dependencies" (which files use which fragments)
- More complex implementation
- Risk of missing edge cases

### Strategy 3: Two-Tier with Debouncing (Recommended)

Combine immediate per-file validation with debounced cross-file refresh:

```
on did_change(file_A):
    // Immediate: validate the changed file (< 1ms with Salsa)
    publish_diagnostics(file_A)

    // Debounced (e.g., 300ms): refresh affected files
    debounce("cross-file-refresh", 300ms, || {
        if file_A is schema:
            refresh_all_document_files()
        else:
            refresh_files_affected_by(file_A)
    })
```

**Why this is recommended:**
- Users get instant feedback on the file they're editing
- Cross-file errors appear within 300ms (imperceptible delay)
- Debouncing coalesces rapid keystrokes (typing a name refactor)
- Salsa memoization makes the debounced refresh cheap when nothing actually changed

---

## Detailed Design for Strategy 3

### What "Affected Files" Means Per Edit Category

Even though we don't need to classify every individual edit, understanding the categories helps optimize the refresh scope:

#### Category 1: Schema Content Changes
**Trigger:** Any edit to a file with `DocumentKind::Schema`
**Affected:** ALL document files (every operation/fragment validates against the schema)
**Reason:** `validate_file()` depends on `merged_schema_with_diagnostics()` which depends on `schema_types()` which depends on `file_type_defs()` for the changed schema file

**Optimization:** Can compare `schema_types()` before/after. If the merged types haven't changed (e.g., editing a comment or whitespace), skip the broadcast.

#### Category 2: Fragment Structure Changes (Name, Type Condition)
**Trigger:** Renaming a fragment, changing its type condition, adding/removing a fragment
**Affected:** Files that spread the changed fragment (directly or transitively)
**How to find them:** `fragment_spreads_index()` gives the reverse dependency graph

#### Category 3: Fragment Body Changes (Selection Set)
**Trigger:** Editing fields inside a fragment body
**Affected:** Files that spread this fragment (the merged validation document changes)
**Scope:** Narrower than structure changes - only affects validation, not name indexes

#### Category 4: Operation Structure Changes (Name, Variables)
**Trigger:** Renaming an operation, changing variables
**Affected:** Other files with same-named operations (uniqueness check)
**How to find them:** `project_operation_name_index()`

#### Category 5: Operation Body Changes
**Trigger:** Editing fields inside an operation body
**Affected:** Only the file itself (operations don't have cross-file dependents)
**Scope:** No cross-file refresh needed

### Implementation Sketch

```rust
// In the LSP did_change handler:
async fn did_change(&self, params: DidChangeTextDocumentParams) {
    // ... existing file update logic ...

    // Immediate: validate changed file
    self.validate_file_with_snapshot(&uri, snapshot.clone()).await;

    // Schedule debounced cross-file refresh
    let is_schema = document_kind == DocumentKind::Schema;
    self.schedule_cross_file_refresh(workspace_uri, project_name, is_schema).await;
}

async fn refresh_affected_files(&self, workspace_uri: &str, project_name: &str) {
    let host = self.workspace.get_host(workspace_uri, project_name);
    let snapshot = host.snapshot().await;

    // Get all loaded file paths
    let all_files = snapshot.all_file_paths();

    // Use all_diagnostics_for_files which merges per-file + project-wide
    let diagnostics_map = snapshot.all_diagnostics_for_files(&all_files);

    for (file_path, diagnostics) in &diagnostics_map {
        let lsp_diagnostics = diagnostics.iter().map(convert_ide_diagnostic).collect();
        self.client.publish_diagnostics(file_uri, lsp_diagnostics, None).await;
    }

    // Clear diagnostics for files that no longer have issues
    for file_path in &all_files {
        if !diagnostics_map.contains_key(file_path) {
            self.client.publish_diagnostics(file_uri, vec![], None).await;
        }
    }
}
```

### Key Consideration: Salsa's "Backdate" Optimization

Salsa has an important optimization: when a tracked function's inputs change but the output doesn't, Salsa "backdates" the output (marks it as unchanged). This means downstream queries don't need to recompute.

For example:
- Edit `schema.graphql` to add a comment (whitespace-only change)
- `file_type_defs(schema_file)` recomputes, but returns the **same** TypeDef map
- Salsa backdates the result -> `schema_types()` doesn't recompute
- `validate_file()` for all document files returns cached results

This means Strategy 1 (broadcast to all files) is actually quite cheap for edits that don't change semantics. The "verify cached" cost is just HashMap comparisons.

### Optimization: Structure Fingerprinting

To make the common case (body-only edits) even cheaper, we could add a fingerprint to `FileStructureData`:

```rust
pub struct FileStructureData {
    pub file_id: FileId,
    pub type_defs: Arc<Vec<TypeDef>>,
    pub operations: Arc<Vec<OperationStructure>>,
    pub fragments: Arc<Vec<FragmentStructure>>,
    // Hash of names/types only - cheap to compare
    pub structure_fingerprint: u64,
}
```

If the fingerprint is the same before and after an edit, we know no structural change occurred and can skip the cross-file refresh entirely. This is effectively the structure/body separation already implemented in the HIR, but surfaced to the LSP layer for decision-making.

---

## Current Structure/Body Split Assessment

The existing split is well-designed at the conceptual level:

| Layer | What It Captures | When It Invalidates |
|-------|-----------------|---------------------|
| `FileStructureData` | Names, types, signatures | Name/type changes only |
| `OperationBody` / `FragmentBody` | Selection sets, spreads | Body content changes |
| `file_type_defs()` | Schema types per file | Schema file content changes |
| `schema_types()` | Merged schema types | Any schema file content change |

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

### Phase 1: Immediate (Get Cross-File Refresh Working)

1. **Add debounced all-file diagnostic refresh after `did_change`.**
   - After updating the changed file's diagnostics immediately, schedule a debounced (200-300ms) refresh of all loaded files
   - Use `all_diagnostics_for_files()` which already handles the merge correctly
   - Salsa memoization makes this cheap for files unaffected by the change

2. **Move project-wide lints from `did_save` to the debounced refresh.**
   - Currently project-wide lints (unused fragments, unused fields) only run on save
   - They should be included in the debounced cross-file refresh for consistency

### Phase 2: Optimization (If Performance Requires It)

3. **Add schema-vs-document classification to the refresh.**
   - Schema edits: refresh all document files (necessary - they all depend on schema)
   - Document edits: only refresh files that share fragment/operation name dependencies

4. **Add structure fingerprinting.**
   - Detect when an edit only changed body content (e.g., editing a selection set)
   - Skip cross-file refresh entirely for body-only edits in document files
   - This handles the common case (typing inside an operation) with zero cross-file cost

5. **Track reverse fragment dependencies.**
   - Build an index: fragment name -> files that spread it
   - On fragment rename/delete, only refresh those specific files
   - This is the finest-grained approach but requires maintaining the reverse index

### Phase 3: Advanced (Future Consideration)

6. **Workspace-level pull diagnostics (LSP 3.17+).**
   - Instead of pushing diagnostics, let the editor pull them
   - The editor asks "what are the diagnostics for file X?" and Salsa gives the current answer
   - Eliminates the push-based refresh problem entirely
   - Requires editor support (VS Code supports this)

---

## Summary

| Question | Answer |
|----------|--------|
| Do we need to classify every edit type? | **No.** Salsa handles fine-grained invalidation automatically. |
| What's missing? | The LSP layer only re-validates the edited file, not affected files. |
| What's the fix? | Debounced cross-file diagnostic refresh after each edit. |
| Is it expensive? | No - Salsa memoization means unaffected files return cached results instantly. |
| Does the structure/body split help? | Yes - it could be used to skip cross-file refresh for body-only edits. |
| Is the current Salsa architecture correct? | Yes - the dependency graph is well-designed. The gap is only in the LSP orchestration layer. |
