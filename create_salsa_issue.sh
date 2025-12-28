#!/bin/bash
set -euo pipefail

# Create GitHub issue for completing Salsa transition
# Run this after authenticating with: gh auth login

TITLE="[Architecture] Complete Salsa-based Incremental Computation Transition"

BODY=$(cat <<'EOF'
## Problem

The project has a partially-implemented Salsa-based incremental computation architecture. Critical pieces are missing or bypassed, preventing the system from achieving its design goals.

**Current State:** Salsa infrastructure exists but validation bypasses it entirely, using apollo-compiler directly instead.

**Evidence:**
- Body queries (`operation_body`, `fragment_body`) documented but don't exist
- Validation re-parses documents instead of using HIR queries
- Database uses `Cell<Option<ProjectFiles>>` instead of proper Salsa inputs
- All diagnostics use `DiagnosticRange::default()` (5 TODOs for missing positions)
- Transitive fragment resolution incomplete
- FileRegistry not integrated with Salsa

## Impact

**Without completing the Salsa transition:**
- ❌ Editing one file re-validates the entire project
- ❌ Schema queries aren't cached between validations
- ❌ Fragment changes trigger full re-validation
- ❌ The "golden invariant" (body edits don't invalidate schema) doesn't work
- ❌ Diagnostics can't show accurate error positions
- ❌ Benchmarks may be measuring HashMap performance, not incremental computation

**The promise:** Edit one operation → only that operation re-validates (milliseconds)
**The reality:** Edit one operation → everything re-validates (potentially seconds for large projects)

## Detailed Implementation Plan

See [`SALSA_COMPLETION_PLAN.md`](./SALSA_COMPLETION_PLAN.md) for the complete 50+ page implementation plan.

### Phase Summary

1. **Fix Database Foundation** (1-2 weeks)
   - Remove `Cell` from RootDatabase
   - Add position tracking to all HIR types
   - Make ProjectFiles a proper Salsa input

2. **Implement Body Queries** (2-3 weeks)
   - Create `operation_body()` and `fragment_body()` queries
   - Implement transitive fragment dependency resolution
   - Handle circular fragment references

3. **Make Validation Use HIR** (2-3 weeks)
   - Build schema from HIR instead of re-parsing
   - Build executable documents from HIR structures + bodies
   - Create incremental validation query
   - Stop all direct apollo-compiler parsing in validation

4. **Integration & Testing** (1-2 weeks)
   - Update AnalysisHost to use new queries
   - Comprehensive testing (unit, integration, benchmarks)
   - Verify golden invariant actually works

5. **Documentation & Cleanup** (1 week)
   - Update docs to reflect reality
   - Remove dead code and workarounds
   - Add performance regression tests to CI

**Total Estimated Timeline:** 7-11 weeks

## Success Criteria

### Must Have (MVP)
- [ ] All diagnostics show correct positions (no `default()`)
- [ ] Validation uses HIR queries (no direct parsing)
- [ ] Body queries exist and work
- [ ] Transitive fragment resolution works
- [ ] Editing one file doesn't re-validate unrelated files

### Should Have (Production Ready)
- [ ] Benchmarks show 10x+ speedup for incremental vs full
- [ ] Golden invariant verified: body edit <100ns for schema query
- [ ] Integration tests with 100+ file projects
- [ ] Performance regression detection in CI

## Why This Matters

This isn't just architecture polish—it's the difference between:
- **Usable:** LSP responds in milliseconds while you type
- **Unusable:** LSP lags for seconds on every edit in large projects

The Salsa architecture was chosen specifically to enable large-project support. Without completing it, we're paying the complexity cost without getting the benefits.

## References

- [Detailed Implementation Plan](./SALSA_COMPLETION_PLAN.md)
- [HIR Crate README](./crates/graphql-hir/README.md) (documents missing features)
- [Rust-Analyzer HIR Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html#HIR) (inspiration)
- [Salsa Documentation](https://github.com/salsa-rs/salsa)

## Related Issues

This blocks or relates to:
- Any performance improvements
- Large project support
- Real-time validation improvements
- Fragment-heavy project support

---

**Note:** This issue represents ~2-3 months of focused work. It's the largest architectural debt in the project. The good news: the foundation exists, we just need to complete what was started.
EOF
)

echo "Creating issue..."
gh issue create \
  --repo trevor-scheer/graphql-lsp \
  --title "$TITLE" \
  --body "$BODY" \
  --label "architecture,performance,salsa"

echo "✓ Issue created successfully"
