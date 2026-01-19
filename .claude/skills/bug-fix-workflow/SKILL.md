---
name: bug-fix-workflow
description: Fix bugs using the two-commit structure with failing test first. Use when fixing bugs, addressing issues, or correcting incorrect behavior.
user-invocable: true
---

# Bug Fix Workflow

Bug fixes in this project use a **two-commit structure** that proves the bug exists before fixing it.

## The Two-Commit Structure

### Commit 1: Reproduce the Bug

```
test: reproduce <issue description>
```

This commit adds a **failing test** that demonstrates the bug:

- The test should fail BEFORE the fix
- The test should pass AFTER the fix
- The test prevents future regressions

### Commit 2: Fix the Bug

```
fix: <description of what was fixed>
```

This commit:

- Fixes the actual bug
- Updates the test if needed (e.g., expected values)
- May include additional related fixes

## Why This Structure?

1. **Proves the bug exists** before the fix
2. **Validates the fix** actually resolves the issue
3. **Prevents regressions** by leaving the test in place
4. **Makes review easier** by separating reproduction from fix

## Workflow Steps

### 1. Understand the Bug

- Read the issue/report carefully
- Identify the expected vs actual behavior
- Determine the root cause

### 2. Write a Failing Test

```rust
#[test]
fn issue_123_fragment_spread_on_wrong_type() {
    // This test reproduces the bug reported in issue #123
    let result = validate("...");

    // Expected: error about type mismatch
    // Actual (bug): no error reported
    assert!(!result.diagnostics.is_empty());
}
```

### 3. Verify the Test Fails

```bash
cargo test issue_123
# Should FAIL - this proves the bug exists
```

### 4. Commit the Failing Test

```bash
git add .
git commit -m "test: reproduce fragment spread type mismatch (issue #123)"
```

### 5. Implement the Fix

Make the minimal changes needed to fix the bug.

### 6. Verify the Test Passes

```bash
cargo test issue_123
# Should PASS now
cargo test  # All tests should pass
```

### 7. Commit the Fix

```bash
git add .
git commit -m "fix: validate fragment spread target type exists in schema

The fragment spread validator was not checking if the target type
existed in the schema before validating field selections."
```

## Commit Message Guidelines

### Test Commit

```
test: reproduce <brief description>

- Reference issue number if applicable
- Describe what the test checks
- Note expected vs actual behavior
```

### Fix Commit

```
fix: <what was fixed>

<Why the bug occurred>
<What the fix does>
<Any side effects or related changes>
```

## Common Mistakes to Avoid

- **Don't combine test and fix** in one commit
- **Don't write a passing test first** - the test must fail initially
- **Don't skip the test** for "obvious" fixes
- **Don't forget to run all tests** before the fix commit

## Checklist

- [ ] Bug understood and root cause identified
- [ ] Failing test written that reproduces the bug
- [ ] Test verified to fail before fix
- [ ] Test committed with `test:` prefix
- [ ] Fix implemented
- [ ] Test verified to pass after fix
- [ ] All tests pass (`cargo test`)
- [ ] Fix committed with `fix:` prefix
- [ ] Clippy is clean (`cargo clippy`)
