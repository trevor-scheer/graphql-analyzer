#!/bin/bash
set -euo pipefail

# PreCompact hook: injects critical context that must survive compaction.
# Ensures important project state is preserved when the context window is compressed.

branch=$(git branch --show-current 2>/dev/null || echo "unknown")

# Detect which crates have been modified in this session
changed_crates=$(git diff --name-only HEAD 2>/dev/null | grep -oP 'crates/[^/]+' | sort -u | tr '\n' ', ' || echo "none")

cat <<EOF
{"additionalContext": "PRESERVE: Working on branch '$branch'. Modified crates: ${changed_crates:-none}. Key invariants: structure/body separation, file isolation, index stability. Use /skills for guided workflows. Always use --repo flag with gh CLI."}
EOF
