#!/bin/bash
# Checks that a changeset exists when user-facing code has been modified.
# Runs as a Claude Code Stop hook.

# Read JSON input from stdin (stop hooks receive context)
input=$(cat)

# Only run in a git repo
if ! git rev-parse --git-dir >/dev/null 2>&1; then
  exit 0
fi

# Determine the base branch to compare against
if git rev-parse origin/main >/dev/null 2>&1; then
  base="origin/main"
elif git rev-parse origin/master >/dev/null 2>&1; then
  base="origin/master"
else
  exit 0
fi

current_branch=$(git branch --show-current)

# Don't check on main/master itself
if [[ "$current_branch" = "main" || "$current_branch" = "master" ]]; then
  exit 0
fi

# Check if there are any commits on this branch vs base
commit_count=$(git rev-list "$base..HEAD" --count 2>/dev/null) || commit_count=0
if [[ "$commit_count" -eq 0 ]]; then
  exit 0
fi

# Check if any commits touch user-facing source code (not just tests, CI, docs)
has_user_facing_changes=false
changed_files=$(git diff --name-only "$base...HEAD" 2>/dev/null)

while IFS= read -r file; do
  case "$file" in
    # User-facing source code
    crates/*/src/*.rs|crates/*/src/**/*.rs)
      has_user_facing_changes=true
      break
      ;;
    editors/vscode/src/*)
      has_user_facing_changes=true
      break
      ;;
  esac
done <<< "$changed_files"

if [[ "$has_user_facing_changes" = "false" ]]; then
  exit 0
fi

# Check if a changeset already exists
changeset_count=$(find .changeset -name '*.md' ! -name 'README*' 2>/dev/null | wc -l)
if [[ "$changeset_count" -gt 0 ]]; then
  exit 0
fi

echo "This branch has user-facing changes but no changeset. Create one in .changeset/ for the changelog." >&2
echo "" >&2
echo "Format:" >&2
echo "  ---" >&2
echo "  package-name: patch|minor|major" >&2
echo "  ---" >&2
echo "" >&2
echo "  Description of the change ([#PR](url))" >&2
echo "" >&2
echo "Packages: graphql-analyzer-cli, graphql-analyzer-lsp, graphql-analyzer-mcp, graphql-analyzer-vscode" >&2
echo "Skip this if the change is internal refactoring, CI-only, or test-only." >&2
exit 2
