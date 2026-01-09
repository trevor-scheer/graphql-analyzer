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

# Download gh binary directly from GitHub releases
# This avoids apt which may not work in restricted network environments
GH_VERSION="2.67.0"
GH_ARCHIVE="gh_${GH_VERSION}_linux_amd64.tar.gz"
GH_URL="https://github.com/cli/cli/releases/download/v${GH_VERSION}/${GH_ARCHIVE}"

cd /tmp
curl -fsSL "$GH_URL" -o "$GH_ARCHIVE"
tar -xzf "$GH_ARCHIVE"
sudo mv "gh_${GH_VERSION}_linux_amd64/bin/gh" /usr/local/bin/gh
sudo chmod +x /usr/local/bin/gh
rm -rf "$GH_ARCHIVE" "gh_${GH_VERSION}_linux_amd64"

echo "✓ gh CLI installed successfully: $(gh --version | head -1)"
