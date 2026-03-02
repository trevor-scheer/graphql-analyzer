#!/bin/bash
set -euo pipefail

# PermissionRequest hook: enforces project guardrails programmatically.
# Denies edits to protected files and blocks dangerous operations.

input=$(cat)

tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null || true)
tool_input=$(echo "$input" | jq -r '.tool_input // empty' 2>/dev/null || true)

# Guard: never edit release.yml
if [[ "$tool_name" == "Edit" || "$tool_name" == "Write" ]]; then
  file_path=$(echo "$tool_input" | jq -r '.file_path // empty' 2>/dev/null || true)
  if [[ "$file_path" == *".github/workflows/release.yml"* ]]; then
    echo '{"hookSpecificOutput":{"hookEventName":"PermissionRequest","permissionDecision":"deny","permissionDecisionReason":"release.yml is auto-managed. Do not edit manually."}}'
    exit 0
  fi
fi

# Guard: never force-push to main/master
if [[ "$tool_name" == "Bash" ]]; then
  command=$(echo "$tool_input" | jq -r '.command // empty' 2>/dev/null || true)
  if echo "$command" | grep -qP 'git\s+push\s+.*--force.*\s+(main|master)'; then
    echo '{"hookSpecificOutput":{"hookEventName":"PermissionRequest","permissionDecision":"deny","permissionDecisionReason":"Force-pushing to main/master is not allowed."}}'
    exit 0
  fi
fi
