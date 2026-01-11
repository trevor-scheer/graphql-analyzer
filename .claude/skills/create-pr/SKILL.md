---
name: create-pr
description: Create pull requests following project standards. Use when opening a PR, preparing changes for review, or running gh pr create.
user-invocable: true
---

# Creating Pull Requests

Follow these standards when creating PRs for this project.

## Before Creating the PR

### 1. Verify All Checks Pass

```bash
cargo fmt --check
cargo clippy
cargo test
```

### 2. Review Your Changes

```bash
git status
git diff main...HEAD
git log main..HEAD --oneline
```

### 3. Ensure Commits Follow Conventions

- `feat:` - New feature
- `fix:` - Bug fix
- `refactor:` - Code restructuring
- `docs:` - Documentation only
- `test:` - Test additions/changes
- `chore:` - Maintenance tasks

## PR Title Guidelines

**Do:**
- Use conventional commit format: `feat: add goto definition for fragments`
- Be specific and descriptive
- Keep under 72 characters

**Don't:**
- Use emoji in titles
- Use vague titles like "Updates" or "Fixes"
- Include issue numbers in the title (put in body)

## PR Description Structure

Use this template:

```markdown
## Summary

<1-3 bullet points explaining what changed and why>

## Changes

- <Specific change 1>
- <Specific change 2>
- <New/updated tests>

## Consulted SME Agents

- **agent-name.md**: Key guidance received
- **another-agent.md**: Relevant advice applied

## Manual Testing Plan

<Steps a reviewer can follow to verify the changes work>
```

## What NOT to Include

**Never mention in PR descriptions:**
- "All tests passing"
- "Clippy is clean"
- "No warnings"
- Any CI-related status

These are enforced by CI and mentioning them adds zero value.

## Manual Testing Plan Section

This section is ONLY for manual verification steps reviewers can follow:

**Good:**
```markdown
## Manual Testing Plan

1. Open a `.graphql` file with a fragment spread
2. Hover over the fragment name
3. Verify type information appears in the tooltip
```

**Bad:**
```markdown
## Manual Testing Plan

- All tests pass
- Ran cargo clippy with no warnings
```

## Creating the PR

Use `gh pr create` with the repo flag:

```bash
gh pr create \
  --repo trevor-scheer/graphql-lsp \
  --head your-branch-name \
  --title "feat: your feature description" \
  --body "$(cat <<'EOF'
## Summary

- Added X feature to improve Y

## Changes

- Implemented X in `crates/graphql-analysis/`
- Added tests for edge cases
- Updated documentation

## Consulted SME Agents

- **lsp.md**: Confirmed response format
- **rust.md**: Advised on error handling

## Manual Testing Plan

1. Step one
2. Step two
EOF
)"
```

## After Creating the PR

1. Verify the PR looks correct on GitHub
2. Check that CI starts running
3. Address any review feedback promptly
4. Update PR title/description if you push additional commits

## Checklist

- [ ] All tests pass locally
- [ ] Clippy is clean
- [ ] Code is formatted
- [ ] Commits follow conventional format
- [ ] PR title is descriptive (no emoji)
- [ ] Summary explains what and why
- [ ] Changes section lists specifics
- [ ] Consulted SME Agents documented
- [ ] Manual Testing Plan has actionable steps
- [ ] No CI status mentioned in description
