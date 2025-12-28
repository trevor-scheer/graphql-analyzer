# Complete Salsa Transition Implementation Plan

## Executive Summary

The project has a partially-implemented Salsa-based incremental computation architecture. The structure is documented and some queries exist, but critical pieces are missing or bypassed. This document outlines exactly what needs to be completed to deliver on the promised architecture.

**Current State:** Salsa infrastructure exists but validation and core features bypass it entirely, using apollo-compiler directly instead.

**Target State:** Full query-based incremental computation where editing one file only revalidates affected dependencies.

---

## Critical Problems Identified

### 1. **Body Queries Don't Exist**
- **Status:** Documented but not implemented
- **Impact:** Cannot achieve fine-grained invalidation (the core benefit of structure/body split)
- **Evidence:** `operation_body()` and `fragment_body()` mentioned in HIR README but don't exist in code

### 2. **Validation Bypasses HIR**
- **Status:** Calls apollo-compiler directly instead of using Salsa queries
- **Impact:** Re-parses documents, re-builds schema, nullifies incremental computation
- **Evidence:** `crates/graphql-analysis/src/validation.rs:84-97`

### 3. **Database State Management is Broken**
- **Status:** Uses `Cell<Option<ProjectFiles>>` instead of Salsa inputs
- **Impact:** Changes don't trigger proper invalidation
- **Evidence:** `crates/graphql-db/src/lib.rs:98`

### 4. **Position Tracking Missing from HIR**
- **Status:** All diagnostics use `DiagnosticRange::default()`
- **Impact:** Errors can't show user where problems are
- **Evidence:** 5 TODOs in `document_validation.rs`

### 5. **Transitive Fragment Dependencies Incomplete**
- **Status:** Only tracks direct fragment spreads
- **Impact:** Fragment changes trigger full project revalidation
- **Evidence:** HIR README acknowledges this limitation

### 6. **FileRegistry Not Integrated**
- **Status:** Separate from Salsa database
- **Impact:** File changes don't properly propagate through query system
- **Evidence:** Default `project_files()` returns `None`

---

## Implementation Phases

### **Phase 1: Fix Database Foundation** (1-2 weeks)

#### 1.1 Remove `Cell` from RootDatabase
**Current (broken):**
```rust
#[salsa::db]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
    project_files: std::cell::Cell<Option<ProjectFiles>>, // ← Wrong
}
```

**Target:**
```rust
#[salsa::db]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

// ProjectFiles should be a regular Salsa input, updated via:
// db.set_project_files(project_files);
```

**Tasks:**
- [ ] Remove `project_files` field from `RootDatabase`
- [ ] Remove `set_project_files()` and `project_files()` methods
- [ ] Make `ProjectFiles` a proper input that queries depend on
- [ ] Update all callers to use Salsa's input system

**Success Criteria:**
- Changing project files automatically invalidates dependent queries
- No interior mutability in database struct

---

#### 1.2 Implement Position Tracking in HIR Types

**Add position information to all structure types:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDef {
    pub name: Arc<str>,
    pub kind: TypeDefKind,
    pub name_range: TextRange,        // ← Add
    pub definition_range: TextRange,   // ← Add
    // ... existing fields
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationStructure {
    pub name: Option<Arc<str>>,
    pub name_range: Option<TextRange>,      // ← Add
    pub operation_range: TextRange,          // ← Add
    // ... existing fields
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentStructure {
    pub name: Arc<str>,
    pub name_range: TextRange,              // ← Add
    pub type_condition_range: TextRange,    // ← Add
    // ... existing fields
}
```

**Tasks:**
- [ ] Add `TextRange` type to HIR (or import from `text-size` crate)
- [ ] Update structure extraction to capture positions from AST
- [ ] Update all HIR types to include position information
- [ ] Remove all `DiagnosticRange::default()` calls
- [ ] Replace with actual positions from HIR

**Success Criteria:**
- All diagnostics show exact error positions
- Zero TODOs for missing positions
- Tests verify position accuracy

---

### **Phase 2: Implement Body Queries** (2-3 weeks)

#### 2.1 Create `operation_body()` Query

```rust
/// Extract the body (selection set) of an operation
/// This query only invalidates when the operation's body changes
#[salsa::tracked]
pub fn operation_body(
    db: &dyn GraphQLHirDatabase,
    operation: OperationStructure,
) -> Arc<OperationBody> {
    // 1. Get file content for this operation
    // 2. Re-parse just this operation (or use cached parse)
    // 3. Extract selection set
    // 4. Return body with fragment spreads
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationBody {
    pub selections: Vec<Selection>,
    pub fragment_spreads: HashSet<Arc<str>>,
    pub variable_usages: HashSet<Arc<str>>,
}
```

**Tasks:**
- [ ] Implement `operation_body()` Salsa query
- [ ] Create `OperationBody` type
- [ ] Extract selection sets from parsed AST
- [ ] Track fragment spreads used in selections
- [ ] Track variable usages in selections
- [ ] Write tests verifying body extraction
- [ ] Write tests verifying invalidation (body change doesn't affect structure)

---

#### 2.2 Create `fragment_body()` Query

```rust
/// Extract the body (selection set) of a fragment
#[salsa::tracked]
pub fn fragment_body(
    db: &dyn GraphQLHirDatabase,
    fragment: FragmentStructure,
) -> Arc<FragmentBody> {
    // Similar to operation_body
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentBody {
    pub selections: Vec<Selection>,
    pub fragment_spreads: HashSet<Arc<str>>, // For transitive deps
}
```

**Tasks:**
- [ ] Implement `fragment_body()` Salsa query
- [ ] Create `FragmentBody` type
- [ ] Handle fragment spread extraction
- [ ] Write tests for fragment bodies

---

#### 2.3 Implement Transitive Fragment Resolution

```rust
/// Get all fragments transitively used by an operation
/// Handles circular references gracefully
#[salsa::tracked]
pub fn operation_all_fragment_deps(
    db: &dyn GraphQLHirDatabase,
    operation: OperationStructure,
) -> Arc<HashSet<Arc<str>>> {
    let mut visited = HashSet::new();
    let mut to_visit = Vec::new();

    // Get direct spreads from operation body
    let body = operation_body(db, operation);
    to_visit.extend(body.fragment_spreads.iter().cloned());

    while let Some(frag_name) = to_visit.pop() {
        if !visited.insert(frag_name.clone()) {
            continue; // Already processed (handles cycles)
        }

        // Get fragment body and add its spreads
        if let Some(frag_structure) = get_fragment_by_name(db, &frag_name) {
            let frag_body = fragment_body(db, frag_structure);
            to_visit.extend(frag_body.fragment_spreads.iter().cloned());
        }
    }

    Arc::new(visited)
}
```

**Tasks:**
- [ ] Implement transitive fragment dependency resolution
- [ ] Handle circular fragment references (detect and break cycles)
- [ ] Add `get_fragment_by_name()` helper query
- [ ] Test with complex fragment graphs
- [ ] Test cycle detection

**Success Criteria:**
- Editing Fragment C body (used by Fragment B used by Operation A) only invalidates affected operations
- Circular references don't cause infinite loops
- Benchmark shows <100ns for unchanged fragment lookups

---

### **Phase 3: Make Validation Use HIR** (2-3 weeks)

#### 3.1 Build Schema from HIR Instead of Re-parsing

**Current (wrong):**
```rust
// validation.rs re-builds schema using apollo-compiler
let schema = crate::merged_schema::merged_schema(db, project_files);
```

**Target:**
```rust
/// Build apollo-compiler Schema from HIR TypeDefs
#[salsa::tracked]
fn schema_from_hir(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: ProjectFiles,
) -> Arc<apollo_compiler::Schema> {
    let types = graphql_hir::schema_types_with_project(db, project_files);

    // Convert HIR TypeDefs to apollo-compiler Schema
    let mut builder = apollo_compiler::Schema::builder();
    for (name, type_def) in types.iter() {
        // Convert HIR → apollo schema type
        builder.add_type(convert_type_def(type_def));
    }
    builder.build()
}
```

**Tasks:**
- [ ] Implement HIR TypeDef → apollo-compiler type conversion
- [ ] Create `schema_from_hir()` query
- [ ] Stop calling apollo-compiler parser for schema
- [ ] Verify schema equivalence with tests

---

#### 3.2 Build Executable Document from HIR

**Current (wrong):**
```rust
// Re-parses the entire document
apollo_compiler::parser::Parser::new()
    .parse_into_executable_builder(doc_text, ...);
```

**Target:**
```rust
/// Build executable document from HIR structures + bodies
#[salsa::tracked]
fn document_from_hir(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
) -> Arc<apollo_compiler::ExecutableDocument> {
    let structure = graphql_hir::file_structure(db, file_id, ...);

    let mut builder = apollo_compiler::ExecutableDocument::builder(...);

    // Add operations with bodies
    for op_structure in &structure.operations {
        let body = graphql_hir::operation_body(db, op_structure.clone());
        builder.add_operation(convert_operation(op_structure, body));
    }

    // Add fragments with bodies
    for frag_structure in &structure.fragments {
        let body = graphql_hir::fragment_body(db, frag_structure.clone());
        builder.add_fragment(convert_fragment(frag_structure, body));
    }

    builder.build()
}
```

**Tasks:**
- [ ] Implement HIR → apollo executable conversion
- [ ] Create conversion functions for operations and fragments
- [ ] Stop re-parsing documents in validation
- [ ] Add fragment dependencies to document automatically

**Success Criteria:**
- Validation uses HIR queries exclusively
- No direct apollo-compiler parsing in validation code
- Benchmark shows warm validation is 100x+ faster than cold

---

#### 3.3 Incremental Validation Query

```rust
/// Validate a single file using HIR and cached schema
#[salsa::tracked]
pub fn validate_file_incremental(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
    project_files: ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    // 1. Get schema from HIR (cached)
    let schema = schema_from_hir(db, project_files);

    // 2. Get document from HIR (uses body queries)
    let document = document_from_hir(db, file_id);

    // 3. Validate (apollo-compiler validation is still used here,
    //    but we're not re-parsing, just validating)
    let errors = apollo_compiler::validate(&document, &schema);

    // 4. Convert to our Diagnostic type with proper positions
    Arc::new(convert_diagnostics(errors))
}
```

**Tasks:**
- [ ] Implement incremental validation query
- [ ] Ensure it only depends on relevant HIR queries
- [ ] Replace current validation with this
- [ ] Add cache hit metrics/logging

---

### **Phase 4: Integration & Testing** (1-2 weeks)

#### 4.1 Update AnalysisHost to Use New Queries

**Tasks:**
- [ ] Remove old non-Salsa validation code paths
- [ ] Update `AnalysisHost` to use `validate_file_incremental()`
- [ ] Ensure file updates properly invalidate through Salsa
- [ ] Remove FileRegistry workarounds

---

#### 4.2 Comprehensive Testing

**Unit Tests:**
- [ ] Test each HIR query independently
- [ ] Test structure extraction with positions
- [ ] Test body extraction with fragment spreads
- [ ] Test transitive fragment resolution
- [ ] Test circular fragment detection

**Integration Tests:**
- [ ] Large project (100+ files) validation
- [ ] File edit → measure what re-validates
- [ ] Fragment change → verify only affected operations re-validate
- [ ] Schema change → verify all documents re-validate

**Benchmark Tests:**
- [ ] Cold vs warm parse (should be 100-1000x difference)
- [ ] Golden invariant (schema query after body edit: <100ns)
- [ ] Fragment resolution warm vs cold
- [ ] Full project validation warm vs cold

---

#### 4.3 Validation Benchmarks

Create a comprehensive benchmark suite:

```rust
// benches/incremental_validation.rs

fn bench_edit_single_operation(c: &mut Criterion) {
    let project = setup_large_project(); // 100+ files

    c.bench_function("edit_single_operation_body", |b| {
        b.iter_batched(
            || {
                let db = project.database.clone();
                // Initial validation (populate cache)
                validate_all_files(&db);
                (db, random_operation_id())
            },
            |(mut db, op_id)| {
                // Edit one operation body
                edit_operation_body(&mut db, op_id);
                // Measure re-validation time
                black_box(validate_all_files(&db))
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_full_revalidation(c: &mut Criterion) {
    // Compare: incremental vs full re-parse
    // Should show massive difference
}
```

**Success Criteria:**
- Editing one operation body re-validates only that operation (<10ms)
- Schema queries remain cached (measured in nanoseconds)
- Fragment changes only re-validate dependent operations
- Incremental validation is 10-100x faster than full re-parse

---

## Phase 5: Documentation & Cleanup (1 week)

### 5.1 Update Documentation
- [ ] Update all READMEs to reflect actual implementation
- [ ] Remove "TODO" sections that are now complete
- [ ] Add architecture diagrams showing query dependencies
- [ ] Document performance characteristics

### 5.2 Remove Dead Code
- [ ] Remove old non-incremental code paths
- [ ] Remove apollo-compiler direct parsing from validation
- [ ] Clean up temporary workarounds
- [ ] Remove all TODOs introduced during transition

### 5.3 Performance Documentation
- [ ] Document expected benchmark results
- [ ] Add regression detection to CI
- [ ] Create performance guide for contributors

---

## Risk Mitigation

### Risk 1: Apollo-compiler API Incompatibility
**Issue:** Converting HIR → apollo types may not be possible/practical

**Mitigation:**
- Validate conversion approach early (Phase 3.1)
- Keep apollo-compiler as validation engine (not parsing)
- If conversion is too complex, consider writing custom validator using HIR

### Risk 2: Salsa Performance Overhead
**Issue:** Query overhead may exceed benefits for small projects

**Mitigation:**
- Benchmark early (Phase 2)
- Add feature flag to disable Salsa if needed
- Document performance characteristics clearly
- Consider hybrid approach (Salsa for large projects, direct for small)

### Risk 3: Breaking Changes During Migration
**Issue:** Refactoring may break existing features

**Mitigation:**
- Keep old code paths until new ones are tested
- Add feature flags for incremental rollout
- Comprehensive integration tests before removal
- Test against real-world projects

---

## Success Metrics

### Must Have (MVP)
- [ ] All diagnostics show correct positions (no `default()`)
- [ ] Validation uses HIR queries (no direct parsing in validation.rs)
- [ ] Body queries exist and work (`operation_body`, `fragment_body`)
- [ ] Transitive fragment resolution works correctly
- [ ] Editing one file doesn't re-validate unrelated files
- [ ] All existing tests pass

### Should Have (Production Ready)
- [ ] Benchmarks show 10x+ speedup for incremental vs full validation
- [ ] Golden invariant verified: body edit doesn't invalidate schema (<100ns)
- [ ] Circular fragment references handled correctly
- [ ] Integration tests with 100+ file projects
- [ ] Performance regression detection in CI

### Nice to Have (Polish)
- [ ] Performance guide for users
- [ ] Architecture documentation with diagrams
- [ ] Comparison with other GraphQL LSPs
- [ ] Blog post explaining the architecture

---

## Estimated Timeline

| Phase | Duration | Blockers |
|-------|----------|----------|
| Phase 1: Database Foundation | 1-2 weeks | None |
| Phase 2: Body Queries | 2-3 weeks | Phase 1 |
| Phase 3: Validation | 2-3 weeks | Phase 2 |
| Phase 4: Integration & Testing | 1-2 weeks | Phase 3 |
| Phase 5: Documentation | 1 week | Phase 4 |
| **Total** | **7-11 weeks** | |

**Parallel Work Opportunities:**
- Documentation can start during Phase 4
- Some tests can be written during implementation phases
- Position tracking (Phase 1.2) can be done alongside Phase 2

---

## Open Questions

1. **Should we keep apollo-compiler for validation?**
   - Pro: Battle-tested, spec-compliant
   - Con: Adds conversion overhead
   - Decision: Keep it but only for validation, not parsing

2. **How to handle TypeScript/JavaScript extraction?**
   - Current approach works but bypasses some Salsa benefits
   - Need to ensure extracted blocks integrate with HIR properly

3. **Feature flag strategy?**
   - Should old code paths remain available?
   - How to gradually roll out new architecture?

4. **Performance targets?**
   - What project size should we optimize for?
   - Is 10x speedup enough or should we target 100x?

---

## References

- [Salsa Documentation](https://github.com/salsa-rs/salsa)
- [Rust-Analyzer HIR Layer](https://rust-analyzer.github.io/book/contributing/architecture.html#HIR)
- [Apollo-Compiler Validation API](https://docs.rs/apollo-compiler/latest/apollo_compiler/validation/)
- Current implementation: `crates/graphql-{db,syntax,hir,analysis}`

---

## Conclusion

The Salsa transition is **60% complete**:
- ✅ Database structure exists
- ✅ Structure extraction works
- ✅ Some queries implemented
- ❌ Body queries missing
- ❌ Validation bypasses HIR
- ❌ Position tracking incomplete
- ❌ Database state management broken

**Core Issue:** The implementation diverged from the architecture. Code was written to work around missing pieces rather than completing them.

**Path Forward:** Complete the missing pieces methodically. Don't add workarounds. Each phase unblocks the next and brings measurable improvements.

**Expected Outcome:** A genuinely incremental GraphQL LSP that only re-validates changed code, with validation times measured in milliseconds instead of seconds for large projects.
