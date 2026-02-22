#!/bin/sh
# Copy the GraphQL config schema from crates/config to the VS Code extension
# This is run during vscode:prepublish to bundle the schema with the extension

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VSCODE_DIR="$(dirname "$SCRIPT_DIR")"
SCHEMA_SRC="$VSCODE_DIR/../../crates/config/schema/graphqlrc.schema.json"
SCHEMA_DEST="$VSCODE_DIR/schema/graphqlrc.schema.json"

mkdir -p "$(dirname "$SCHEMA_DEST")"
cp "$SCHEMA_SRC" "$SCHEMA_DEST"

echo "Copied schema to $SCHEMA_DEST"
