#!/bin/sh
# GraphQL Analyzer VSCode/Cursor Extension Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install-vscode.sh | sh

set -e

REPO="trevor-scheer/graphql-analyzer"

# Find editor CLI (prefer cursor if both available, or use EDITOR_CLI env var)
find_editor() {
    if [ -n "$EDITOR_CLI" ]; then
        if command -v "$EDITOR_CLI" >/dev/null 2>&1; then
            EDITOR="$EDITOR_CLI"
            return
        else
            echo "Error: specified EDITOR_CLI '$EDITOR_CLI' not found."
            exit 1
        fi
    fi

    if command -v cursor >/dev/null 2>&1; then
        EDITOR="cursor"
    elif command -v code >/dev/null 2>&1; then
        EDITOR="code"
    else
        echo "Error: neither 'code' nor 'cursor' command found."
        echo "Please install VSCode or Cursor and ensure the CLI is in your PATH."
        echo "In VSCode/Cursor: Cmd+Shift+P > 'Shell Command: Install ... command in PATH'"
        exit 1
    fi
}

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

find_editor
echo "GraphQL Analyzer Extension Installer"
echo "====================================="
echo
echo "Using: $EDITOR"

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
$EDITOR --install-extension "$VSIX_PATH"

echo
echo "Done! Reload $EDITOR to activate the extension."
