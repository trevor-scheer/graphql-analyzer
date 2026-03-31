---
name: create-pr
description: Create pull requests following project standards. Use when opening a PR, preparing changes for review, or running gh pr create.
user-invocable: true
allowed-tools: Bash(gh *), Bash(git *), Bash(cargo fmt *), Bash(cargo clippy *), Bash(cargo test *), Bash(npm *), Bash(cd docs *), Bash(knope *), Read, Edit, Write, Grep, Glob
---

# Creating Pull Requests

Follow these standards when creating PRs for this project.

## Before Creating the PR

### 1. Verify All Checks Pass

**Rust checks:**

```bash
cargo fmt --check
cargo clippy
cargo test
```

**npm checks** (from repo root):

```bash
npm run lint
npm run fmt:check
npm run typecheck
```

**Docs site** (if any `docs/` files changed):

```bash
cd docs && npm run build
```

Fix any issues before proceeding. All of these are enforced by CI — catching them locally avoids failed checks on the PR.

### 2. Update Documentation

Before creating the PR, review whether your changes require documentation updates. Check the diff (`git diff main...HEAD`) and apply the rules below.

**Docs site** (`docs/src/content/docs/`):

| What changed | What to update |
| --- | --- |
| New lint rule | Create `docs/src/content/docs/rules/<rule-name>.mdx` and add it to the sidebar in `docs/astro.config.mjs`. Update `docs/src/content/docs/rules/catalog.mdx`. |
| Changed lint rule behavior/options | Update the rule's `.mdx` in `docs/src/content/docs/rules/` |
| New/changed CLI command or flag | Update the relevant page in `docs/src/content/docs/cli/` |
| New/changed IDE feature | Update the relevant page in `docs/src/content/docs/ide-features/` |
| New/changed configuration option | Update the relevant page in `docs/src/content/docs/configuration/` |
| New/changed editor setup | Update the relevant page in `docs/src/content/docs/editors/` |

**Crate READMEs** (`crates/*/README.md`):

Update a crate's README when you change its public API, add/remove major functionality, or change how it's intended to be used. Internal refactoring doesn't need README updates.

**Root README** (`README.md`):

Update when adding new top-level features, changing installation instructions, or modifying the project's public-facing description.

**Skip documentation updates for:** internal refactoring, test-only changes, CI changes, dependency bumps.

### 3. Create a Changeset (if needed)

For user-facing changes (features, bug fixes, breaking changes), create a changeset:

```bash
knope document-change
```

**Important:** After creating the changeset file, edit it to include a PR link at the end of the first line. Since you're creating a PR, use the PR number you expect (or update after PR is created):

```markdown
---
graphql-analyzer-cli: patch
---

Fix argument parsing bug ([#123](https://github.com/trevor-scheer/graphql-analyzer/pull/123))
```

The PR link helps users trace changelog entries back to implementation details.

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

- [ ] Rust checks pass (`cargo fmt --check`, `cargo clippy`, `cargo test`)
- [ ] npm checks pass (`npm run lint`, `npm run fmt:check`, `npm run typecheck`)
- [ ] Docs site builds (`cd docs && npm run build`) — if docs/ files changed
- [ ] Documentation updated (docs site, crate READMEs, root README) — if user-facing behavior changed
- [ ] Changeset created with PR link (if user-facing changes)
- [ ] Commits follow conventional format
- [ ] PR title is descriptive (no emoji)
- [ ] PR body follows `.github/PULL_REQUEST_TEMPLATE.md` structure
- [ ] No CI status mentioned in description
