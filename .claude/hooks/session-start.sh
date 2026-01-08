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

# Try multiple installation methods in order of reliability

# Method 1: apt-get (preferred for Ubuntu)
install_via_apt() {
  echo "Trying apt-get installation..."
  if sudo apt-get update -qq && sudo apt-get install -y -qq gh 2>/dev/null; then
    return 0
  fi
  return 1
}

# Method 2: go install (if Go is available)
install_via_go() {
  if ! command -v go &> /dev/null; then
    return 1
  fi
  echo "Trying go install..."
  if go install github.com/cli/cli/v2/cmd/gh@latest 2>/dev/null; then
    # Add Go bin to PATH for current session
    export PATH="$PATH:$(go env GOPATH)/bin"
    if command -v gh &> /dev/null; then
      # Ensure GOPATH/bin is in PATH for future commands
      echo "export PATH=\"\$PATH:$(go env GOPATH)/bin\"" >> ~/.bashrc 2>/dev/null || true
      return 0
    fi
  fi
  return 1
}

# Method 3: Direct download from GitHub releases
install_via_download() {
  echo "Trying direct download..."
  local version="2.63.2"
  local url="https://github.com/cli/cli/releases/download/v${version}/gh_${version}_linux_amd64.tar.gz"
  local tmp_dir=$(mktemp -d)

  if curl -fsSL "$url" -o "$tmp_dir/gh.tar.gz" 2>/dev/null; then
    tar -xzf "$tmp_dir/gh.tar.gz" -C "$tmp_dir"
    if sudo mv "$tmp_dir/gh_${version}_linux_amd64/bin/gh" /usr/local/bin/ 2>/dev/null; then
      rm -rf "$tmp_dir"
      return 0
    fi
  fi
  rm -rf "$tmp_dir"
  return 1
}

# Try each method in sequence
if install_via_apt; then
  echo "✓ gh CLI installed via apt: $(gh --version | head -1)"
  exit 0
fi

if install_via_go; then
  echo "✓ gh CLI installed via go: $(gh --version | head -1)"
  exit 0
fi

if install_via_download; then
  echo "✓ gh CLI installed via download: $(gh --version | head -1)"
  exit 0
fi

echo "⚠ Could not install gh CLI - all methods failed"
echo "  This may be due to network restrictions in this environment."
echo "  GitHub operations will need to use git commands directly."
exit 0  # Don't fail the hook - gh is optional
