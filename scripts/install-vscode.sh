#!/bin/sh
# GraphQL Analyzer VSCode Extension Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install-vscode.sh | sh

set -e

REPO="trevor-scheer/graphql-analyzer"

# Check for code CLI
if ! command -v code >/dev/null 2>&1; then
    echo "Error: 'code' command not found."
    echo "Please install VSCode and ensure 'code' is in your PATH."
    echo "In VSCode: Cmd+Shift+P > 'Shell Command: Install code command in PATH'"
    exit 1
fi

# Get latest vscode extension version
get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases" |
        grep -o '"tag_name": *"vscode/v[^"]*"' |
        head -1 |
        sed 's/.*"vscode\/v\([^"]*\)".*/\1/')

    if [ -z "$VERSION" ]; then
        echo "Failed to get latest version"
        exit 1
    fi
}

echo "GraphQL Analyzer VSCode Extension Installer"
echo "============================================"
echo

get_latest_version
echo "Latest version: $VERSION"
echo

URL="https://github.com/${REPO}/releases/download/vscode/v${VERSION}/graphql-analyzer-${VERSION}.vsix"

echo "Downloading extension..."

TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

VSIX_PATH="$TMP_DIR/graphql-analyzer.vsix"

if ! curl -fsSL "$URL" -o "$VSIX_PATH"; then
    echo "Failed to download from ${URL}"
    exit 1
fi

echo "Installing extension..."
code --install-extension "$VSIX_PATH"

echo
echo "Done! Reload VSCode to activate the extension."
