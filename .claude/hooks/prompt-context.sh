#!/bin/bash
set -euo pipefail

# UserPromptSubmit hook: injects branch context and issue details into every prompt.
# Outputs JSON with additionalContext that Claude sees before processing the prompt.

input=$(cat)

context_parts=()

# Add current branch info
branch=$(git branch --show-current 2>/dev/null || echo "unknown")
if [[ "$branch" != "main" && "$branch" != "master" && "$branch" != "unknown" ]]; then
  commit_count=$(git rev-list origin/main..HEAD --count 2>/dev/null || echo "0")
  context_parts+=("Branch: $branch ($commit_count commits ahead of main)")
fi

# Detect issue references in the prompt (e.g., #123, issue 123)
prompt_text=$(echo "$input" | jq -r '.prompt // empty' 2>/dev/null || true)
issue_numbers=$(echo "$prompt_text" | grep -oP '(?:#|issue\s+)\K\d+' 2>/dev/null || true)

for issue_num in $issue_numbers; do
  issue_info=$(gh issue view "$issue_num" --repo trevor-scheer/graphql-analyzer --json title,body,labels --jq '"\(.title) [labels: \(.labels | map(.name) | join(", "))]"' 2>/dev/null || true)
  if [[ -n "$issue_info" ]]; then
    context_parts+=("Issue #$issue_num: $issue_info")
  fi
done

# Output additional context if we have any
if [[ ${#context_parts[@]} -gt 0 ]]; then
  context=$(printf '%s\n' "${context_parts[@]}")
  jq -n --arg ctx "$context" '{"additionalContext": $ctx}'
fi
