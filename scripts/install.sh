#!/bin/sh
# GraphQL Analyzer Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.sh | sh

set -e

REPO="trevor-scheer/graphql-analyzer"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)  OS="unknown-linux-gnu" ;;
        Darwin) OS="apple-darwin" ;;
        *)      echo "Unsupported OS: $OS"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64)  ARCH="x86_64" ;;
        aarch64) ARCH="aarch64" ;;
        arm64)   ARCH="aarch64" ;;
        *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac

    PLATFORM="${ARCH}-${OS}"
}

# Get latest release version
get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases" |
        grep -o '"tag_name": *"cli/v[^"]*"' |
        head -1 |
        sed 's/.*"cli\/v\([^"]*\)".*/\1/')

    if [ -z "$VERSION" ]; then
        echo "Failed to get latest version"
        exit 1
    fi
}

# Download and install a binary
install_binary() {
    BINARY_NAME="$1"
    ASSET_PREFIX="$2"

    URL="https://github.com/${REPO}/releases/download/cli/v${VERSION}/${ASSET_PREFIX}-${PLATFORM}.tar.xz"

    echo "Downloading ${BINARY_NAME}..."

    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT

    if ! curl -fsSL "$URL" -o "$TMP_DIR/archive.tar.xz"; then
        echo "Failed to download ${BINARY_NAME} from ${URL}"
        return 1
    fi

    tar -xJf "$TMP_DIR/archive.tar.xz" -C "$TMP_DIR"

    mkdir -p "$INSTALL_DIR"
    mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"

    echo "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
}

main() {
    echo "GraphQL Analyzer Installer"
    echo "=========================="
    echo

    detect_platform
    echo "Detected platform: $PLATFORM"

    get_latest_version
    echo "Latest version: $VERSION"
    echo

    # Install all binaries
    install_binary "graphql" "graphql-cli"
    install_binary "graphql-lsp" "graphql-lsp"
    install_binary "graphql-mcp" "graphql-mcp"

    echo
    echo "Installation complete!"
    echo

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo "Add the following to your shell profile:"
            echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
            echo
            ;;
    esac

    echo "Run 'graphql --help' to get started."
}

main
