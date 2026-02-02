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

### 2. Create a Changeset (if needed)

For user-facing changes (features, bug fixes, breaking changes), create a changeset:

```bash
knope document-change
```

**Skip changesets for:** internal refactoring, CI changes, test-only changes, documentation updates.

### 4. Review Your Changes

```bash
git status
git diff main...HEAD
git log main..HEAD --oneline
```

### 5. Ensure Commits Follow Conventions

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

## PR Description

**Read the PR template** from `.github/PULL_REQUEST_TEMPLATE.md` and fill in each section based on the changes being made. The template is the single source of truth for PR structure.

### Section Guidelines

**Summary:** 1-3 bullet points explaining what changed and why

**Changes:** Specific list of changes made

**Consulted SME Agents:** List which agents from `.claude/agents/` were consulted and key guidance received. Write "N/A" if none were consulted.

**Manual Testing Plan:** Steps a reviewer can follow to manually verify. Write "N/A" if no manual testing needed.

**Related Issues:** Link issues with `Fixes #123` or `Closes #456`. Leave empty if none.

### What NOT to Include

**Never mention in PR descriptions:**

- "All tests passing"
- "Clippy is clean"
- "No warnings"
- Any CI-related status

These are enforced by CI and mentioning them adds zero value.

## Creating the PR

1. Read `.github/PULL_REQUEST_TEMPLATE.md` to get the current template structure
2. Fill in each section based on the changes
3. Use `gh pr create` with the filled-in body:

```bash
gh pr create \
  --repo trevor-scheer/graphql-analyzer \
  --head your-branch-name \
  --title "feat: your feature description" \
  --body "$(cat <<'EOF'
<filled-in template content here>
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
- [ ] Changeset created (if user-facing changes)
- [ ] Commits follow conventional format
- [ ] PR title is descriptive (no emoji)
- [ ] PR body follows `.github/PULL_REQUEST_TEMPLATE.md` structure
- [ ] No CI status mentioned in description
