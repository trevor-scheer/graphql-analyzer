#!/bin/bash
set -euo pipefail

# PermissionRequest hook: enforces project guardrails programmatically.
# Blocks dangerous operations.

input=$(cat)

tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null || true)
tool_input=$(echo "$input" | jq -r '.tool_input // empty' 2>/dev/null || true)

# Guard: never force-push to main/master
if [[ "$tool_name" == "Bash" ]]; then
  command=$(echo "$tool_input" | jq -r '.command // empty' 2>/dev/null || true)
  if echo "$command" | grep -qP 'git\s+push\s+.*--force.*\s+(main|master)'; then
    echo '{"hookSpecificOutput":{"hookEventName":"PermissionRequest","permissionDecision":"deny","permissionDecisionReason":"Force-pushing to main/master is not allowed."}}'
    exit 0
  fi
fi
