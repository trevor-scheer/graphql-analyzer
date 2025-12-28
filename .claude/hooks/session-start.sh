#!/bin/bash
set -euo pipefail

# Only run in Claude Code on the web remote environments
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# Check if gh is already installed
if command -v gh &> /dev/null; then
  echo "✓ gh CLI already installed: $(gh --version | head -1)"
  exit 0
fi

echo "Installing gh CLI..."

# Install gh from default Ubuntu repositories
# gh is available in Ubuntu 24.04 default repos
sudo apt-get update && sudo apt-get install -y gh

echo "✓ gh CLI installed successfully: $(gh --version | head -1)"
