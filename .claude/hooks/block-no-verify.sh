#!/bin/sh
#
# PreToolUse hook: block --no-verify flag in git commands.
# Git hooks enforce lint/fmt/test checks and must not be bypassed.
#

command=$(echo "$CLAUDE_TOOL_INPUT" | jq -r '.command // empty')

if echo "$command" | grep -q -- '--no-verify\|-n '; then
    # Only block -n for git commit (not other commands where -n means something else)
    if echo "$command" | grep -q -- '--no-verify'; then
        echo "BLOCKED: --no-verify is not allowed. Git hooks must not be bypassed." >&2
        exit 2
    fi
    if echo "$command" | grep -qE 'git commit.*-n\b'; then
        echo "BLOCKED: git commit -n (--no-verify) is not allowed. Git hooks must not be bypassed." >&2
        exit 2
    fi
fi
