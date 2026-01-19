#!/bin/bash
# Generate MCP configuration for Claude Desktop or other MCP clients
#
# Usage:
#   ./scripts/setup-mcp-config.sh [workspace]
#
# This script outputs JSON configuration that can be added to:
#   - Claude Desktop: ~/Library/Application Support/Claude/claude_desktop_config.json (macOS)
#   - Claude Desktop: %APPDATA%\Claude\claude_desktop_config.json (Windows)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default to test-workspace if no argument provided
WORKSPACE="${1:-$REPO_ROOT/test-workspace}"

# Resolve relative paths
if [[ ! "$WORKSPACE" = /* ]]; then
    WORKSPACE="$REPO_ROOT/$WORKSPACE"
fi

# Check if binary exists
if [[ ! -f "$REPO_ROOT/target/debug/graphql" ]]; then
    echo "Error: graphql binary not found. Run 'cargo build' first." >&2
    exit 1
fi

cat <<EOF
Add the following to your MCP client configuration:

{
  "mcpServers": {
    "graphql": {
      "command": "$REPO_ROOT/target/debug/graphql",
      "args": ["mcp", "--workspace", "$WORKSPACE"],
      "env": {
        "RUST_LOG": "graphql_mcp=info"
      }
    }
  }
}

For Claude Desktop on macOS, merge this into:
  ~/Library/Application Support/Claude/claude_desktop_config.json

For Claude Desktop on Windows, merge this into:
  %APPDATA%\\Claude\\claude_desktop_config.json

Available test workspaces:
  - $REPO_ROOT/test-workspace           (multi-project: pokemon, starwars, countries)
  - $REPO_ROOT/test-workspace/pokemon   (single project)
  - $REPO_ROOT/test-workspace/starwars  (single project)
  - $REPO_ROOT/test-workspace/countries (remote schema via introspection)

Tip: Set RUST_LOG=graphql_mcp=debug for verbose logging.
EOF
