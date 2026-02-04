#!/bin/bash
set -euo pipefail

# Only run in Claude Code on the web remote environments
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# Check if gh is already installed
if command -v gh &> /dev/null; then
  echo "✓ gh CLI already installed: $(gh --version | head -1)"
else
  echo "Installing gh CLI..."

  # Install to user's local bin directory (no sudo required)
  LOCAL_BIN="${HOME}/.local/bin"
  mkdir -p "$LOCAL_BIN"

  # Download gh binary directly from GitHub releases
  GH_VERSION=$(curl -fsSL https://api.github.com/repos/cli/cli/releases/latest | grep -oP '"tag_name":\s*"v\K[^"]+')
  GH_ARCHIVE="gh_${GH_VERSION}_linux_amd64.tar.gz"
  GH_URL="https://github.com/cli/cli/releases/download/v${GH_VERSION}/${GH_ARCHIVE}"

  cd /tmp
  curl -fsSL "$GH_URL" -o "$GH_ARCHIVE"
  tar -xzf "$GH_ARCHIVE"
  mv "gh_${GH_VERSION}_linux_amd64/bin/gh" "$LOCAL_BIN/gh"
  chmod +x "$LOCAL_BIN/gh"
  rm -rf "$GH_ARCHIVE" "gh_${GH_VERSION}_linux_amd64"

  if [[ ":$PATH:" != *":$LOCAL_BIN:"* ]]; then
    export PATH="$LOCAL_BIN:$PATH"
  fi

  echo "✓ gh CLI installed successfully: $($LOCAL_BIN/gh --version | head -1)"
fi

# Install npm dependencies (for oxfmt and pre-commit hooks)
if [ ! -d "node_modules" ]; then
  echo ""
  echo "Installing npm dependencies..."
  npm ci --silent
  echo "✓ npm dependencies installed"
fi

# Ensure git hooks are installed (cargo-husky installs on cargo test)
if [ ! -f ".git/hooks/pre-commit" ] || ! grep -q "cargo-husky" ".git/hooks/pre-commit" 2>/dev/null; then
  echo ""
  echo "Installing git hooks..."
  cargo test -p husky-hooks --quiet 2>/dev/null || true
  echo "✓ git hooks installed"
fi

# Show project context
echo ""
echo "Recent commits:"
git log --oneline -5 2>/dev/null || true

echo ""
echo "Open issues:"
gh issue list --repo trevor-scheer/graphql-analyzer --limit 5 2>/dev/null || echo "  (unable to fetch)"

echo ""
echo "Open PRs:"
gh pr list --repo trevor-scheer/graphql-analyzer --limit 5 2>/dev/null || echo "  (unable to fetch)"
