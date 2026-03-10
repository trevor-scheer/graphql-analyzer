#!/bin/sh
# GraphQL Analyzer Binary Installer
#
# Install the latest CLI:
#   curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.sh | sh
#
# Install a specific tool:
#   curl -fsSL .../install.sh | sh -s -- lsp
#   curl -fsSL .../install.sh | sh -s -- mcp
#
# Install a specific version:
#   curl -fsSL .../install.sh | sh -s -- cli 0.1.6
#
# Environment variables:
#   INSTALL_DIR  - Override install directory (default: $HOME/.local/bin)

set -e

REPO="trevor-scheer/graphql-analyzer"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

TOOL="${1:-cli}"
VERSION="${2:-}"

usage() {
    echo "Usage: install.sh [tool] [version]"
    echo ""
    echo "Tools: cli (default), lsp, mcp"
    echo "Version: optional, defaults to latest"
    echo ""
    echo "Examples:"
    echo "  install.sh              # latest CLI"
    echo "  install.sh lsp          # latest LSP"
    echo "  install.sh cli 0.1.6    # specific version"
}

# Map tool name to release tag prefix, artifact prefix, and binary name
resolve_tool() {
    case "$TOOL" in
        cli)
            TAG_PREFIX="graphql-analyzer-cli"
            ARTIFACT_PREFIX="graphql-cli"
            BINARY_NAME="graphql"
            ;;
        lsp)
            TAG_PREFIX="graphql-analyzer-lsp"
            ARTIFACT_PREFIX="graphql-lsp"
            BINARY_NAME="graphql-lsp"
            ;;
        mcp)
            TAG_PREFIX="graphql-analyzer-mcp"
            ARTIFACT_PREFIX="graphql-mcp"
            BINARY_NAME="graphql-mcp"
            ;;
        -h|--help|help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown tool: $TOOL"
            echo "Valid tools: cli, lsp, mcp"
            exit 1
            ;;
    esac
}

detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)  OS="unknown-linux-gnu" ;;
        Darwin) OS="apple-darwin" ;;
        *)      echo "Unsupported OS: $OS (use the PowerShell script for Windows)"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64)  ARCH="x86_64" ;;
        aarch64) ARCH="aarch64" ;;
        arm64)   ARCH="aarch64" ;;
        *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac

    PLATFORM="${ARCH}-${OS}"
}

get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases" |
        grep -o "\"tag_name\": *\"${TAG_PREFIX}/v[^\"]*\"" |
        head -1 |
        sed "s/.*\"${TAG_PREFIX}\/v\([^\"]*\)\".*/\1/")

    if [ -z "$VERSION" ]; then
        echo "Failed to find latest ${TOOL} release"
        exit 1
    fi
}

main() {
    resolve_tool
    detect_platform

    echo "GraphQL Analyzer Installer"
    echo "=========================="
    echo ""
    echo "Tool:     $TOOL ($BINARY_NAME)"
    echo "Platform: $PLATFORM"

    if [ -z "$VERSION" ]; then
        get_latest_version
    fi
    echo "Version:  $VERSION"
    echo ""

    URL="https://github.com/${REPO}/releases/download/${TAG_PREFIX}/v${VERSION}/${ARTIFACT_PREFIX}-${PLATFORM}.tar.xz"

    echo "Downloading ${BINARY_NAME}..."

    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT

    if ! curl -fsSL "$URL" -o "$TMP_DIR/archive.tar.xz"; then
        echo "Failed to download from ${URL}"
        echo ""
        echo "Check that version ${VERSION} exists at:"
        echo "  https://github.com/${REPO}/releases/tag/${TAG_PREFIX}/v${VERSION}"
        exit 1
    fi

    tar -xJf "$TMP_DIR/archive.tar.xz" -C "$TMP_DIR"

    mkdir -p "$INSTALL_DIR"
    mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"

    echo "Installed $BINARY_NAME to ${INSTALL_DIR}/${BINARY_NAME}"
    echo ""

    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo "Add the following to your shell profile:"
            echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
            echo ""
            ;;
    esac

    echo "Run '${BINARY_NAME} --help' to get started."
}

main
