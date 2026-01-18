#!/bin/bash
# Development script for testing the GraphQL MCP server
#
# Usage:
#   ./scripts/mcp-dev.sh [workspace]
#
# Examples:
#   ./scripts/mcp-dev.sh                          # Uses test-workspace
#   ./scripts/mcp-dev.sh test-workspace/pokemon   # Uses pokemon project
#   ./scripts/mcp-dev.sh /path/to/project         # Uses custom path

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default to test-workspace if no argument provided
WORKSPACE="${1:-$REPO_ROOT/test-workspace}"

# Resolve relative paths
if [[ ! "$WORKSPACE" = /* ]]; then
    WORKSPACE="$REPO_ROOT/$WORKSPACE"
fi

echo "Building graphql-cli..."
cargo build --package graphql-cli

echo ""
echo "Starting MCP server for workspace: $WORKSPACE"
echo "Press Ctrl+C to stop"
echo ""

exec "$REPO_ROOT/target/debug/graphql" mcp --workspace "$WORKSPACE"
