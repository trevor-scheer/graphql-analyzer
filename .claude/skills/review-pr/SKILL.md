---
name: review-pr
description: Review pull requests against project standards. Use when reviewing PRs, checking code quality, or providing feedback on changes.
user-invocable: true
---

# Reviewing Pull Requests

Follow this checklist when reviewing PRs for this project.

## Usage

```
/review-pr 123
```

Or just ask: "Review PR #123"

## Review Process

### 1. Fetch PR Information

```bash
gh pr view <number> --repo trevor-scheer/graphql-lsp
gh pr diff <number> --repo trevor-scheer/graphql-lsp
```

### 2. Understand the Changes

- What problem does this PR solve?
- What approach was taken?
- Are there alternative approaches?

### 3. Run the Checklist

## Code Quality Checklist

### Architecture & Design

- [ ] Changes are in the correct layer (db → syntax → hir → analysis → ide → lsp)
- [ ] Follows existing patterns in the codebase
- [ ] No unnecessary abstractions or over-engineering
- [ ] Respects the Golden Invariant (body edits don't invalidate structure)

### Correctness

- [ ] Logic is correct and handles edge cases
- [ ] Error handling is appropriate
- [ ] No obvious bugs or regressions
- [ ] Works for both `.graphql` files and embedded GraphQL in TS/JS

### Testing

- [ ] New functionality has tests
- [ ] Bug fixes include regression tests
- [ ] Tests are readable and well-named
- [ ] Edge cases are covered

### Performance

- [ ] No O(n) operations where O(1) is possible
- [ ] Salsa queries have appropriate granularity
- [ ] No unnecessary allocations in hot paths
- [ ] Large files/projects won't cause issues

### Code Style

- [ ] Follows Rust conventions
- [ ] No unnecessary comments (code is self-documenting)
- [ ] Useful comments explain "why", not "what"
- [ ] Lines under 100 characters where reasonable

### Security

- [ ] No command injection vulnerabilities
- [ ] No path traversal issues
- [ ] Sensitive data not logged

## PR Description Checklist

- [ ] Title is clear and descriptive (no emoji)
- [ ] Summary explains what changed and why
- [ ] Changes section lists specific modifications
- [ ] SME agents consulted and documented
- [ ] Manual testing plan has actionable steps
- [ ] **NO mention of CI status** (tests passing, clippy clean)

## Bug Fix PRs

Bug fixes should use two-commit structure:

1. **First commit**: Failing test reproducing the bug
2. **Second commit**: Fix + any test updates

Verify:
- [ ] First commit's test actually fails without the fix
- [ ] Second commit makes the test pass
- [ ] No other tests broken

## Review Comments

### Approve When

- All checklist items pass
- No blocking issues found
- Minor suggestions can be addressed in follow-up

### Request Changes When

- Missing tests for new functionality
- Correctness issues or bugs
- Security vulnerabilities
- Significant architecture concerns

### Comment (No Decision) When

- Questions need answers before deciding
- Want discussion on approach
- Minor suggestions only

## Example Review Comment

```markdown
## Review Summary

**Overall**: Approve with minor suggestions

### What I Reviewed
- Changes to `crates/graphql-analysis/src/validation.rs`
- New tests in `tests/validation_test.rs`
- PR description and commit history

### Checklist Results
- [x] Architecture: Correct layer, follows patterns
- [x] Correctness: Logic looks sound
- [x] Testing: Good coverage
- [x] Performance: No concerns
- [ ] Code style: Minor suggestion below

### Suggestions

1. **Line 45**: Consider using `if let` instead of `match` for single-arm case
2. **Tests**: Could add test for empty input case

### Questions

None - ready to merge after addressing suggestions.
```

## SME Agents to Consult

When reviewing specific areas:

- **graphql.md**: GraphQL spec compliance in validation logic
- **rust.md**: Idiomatic Rust patterns
- **salsa.md**: Incremental computation correctness
- **lsp.md**: Protocol compliance
