#!/bin/bash
# Auto-format Rust files after edits
# Runs silently - only shows output on error

# Check if the edited file was a Rust file
if [[ "$TOOL_INPUT" == *".rs"* ]]; then
  cargo fmt --quiet 2>/dev/null || true
fi
