# WIP: GraphQL Config Integration and TypeScript Support

## Current Status

In the middle of implementing full GraphQL config loading and TypeScript/JavaScript support in the LSP server.

## What's Working

1. ✅ Extension activates only when `graphql.config.yaml` is found
2. ✅ Extension watches TypeScript/JavaScript files (`.ts`, `.tsx`, `.js`, `.jsx`)
3. ✅ Test workspace with proper config, schema, and example files
4. ✅ Pure `.graphql` file validation with accurate line/column positions

## What's In Progress (Incomplete)

The LSP server code has been partially rewritten to:
- Load GraphQL config from workspace folders
- Store workspace roots and projects from config
- Validate both pure GraphQL and TypeScript files with embedded GraphQL
- Extract GraphQL from `gql` tags in TypeScript/JavaScript

### Current Code State

**Server struct updated** with:
- `init_workspace_folders` - stores workspace folders from initialization
- `workspace_roots` - maps workspace URI to path
- `projects` - maps workspace URI to GraphQL projects loaded from config

**Methods implemented** (but not fully wired up):
- `load_workspace_config()` - finds and loads graphql.config.yaml, creates projects, loads schemas
- `find_project_for_document()` - finds which project a document belongs to
- `validate_document()` - routes to GraphQL or TypeScript validation
- `validate_graphql_document()` - validates pure GraphQL files
- `validate_typescript_document()` - extracts and validates embedded GraphQL (uses tempfile)
- `convert_diagnostics()` - converts apollo-compiler diagnostics to LSP format

**What Needs Completion:**

1. **Initialize handler** - stores workspace folders from InitializeParams ✅
2. **Initialized handler** - needs to iterate through `init_workspace_folders` and call `load_workspace_config()` for each
3. **did_open handler** - needs to be updated to call `validate_document()` instead of using hardcoded schema
4. **did_change handler** - needs to be updated to call `validate_document()`
5. **did_close handler** - should clear diagnostics

## Compilation Issues to Fix

The code doesn't compile yet due to:
1. Need to add `use tower_lsp_server::UriExt` for `to_file_path()` method
2. The old `did_open` and `did_change` handlers still reference removed `documents` DashMap and `DocumentState` struct

## Next Steps

1. Update `initialized()` handler to load configs from stored workspace folders
2. Update `did_open()` and `did_change()` handlers to use new `validate_document()` method
3. Remove old hardcoded schema code
4. Fix compilation errors
5. Test with:
   - `test-workspace/example.graphql` - should show errors for invalid fields
   - `test-workspace/example.tsx` - should validate embedded GraphQL queries

## Files Modified

- `crates/graphql-lsp/src/server.rs` - major rewrite in progress
- `crates/graphql-lsp/Cargo.toml` - added `graphql-extract` and `tempfile` dependencies
- `editors/vscode/src/extension.ts` - added TypeScript/JavaScript language selectors
- `editors/vscode/package.json` - config-based activation events
- `test-workspace/` - added config, schema, and TypeScript example

## Testing Once Complete

```bash
# Build LSP server
cargo build --package graphql-lsp

# Compile extension
cd editors/vscode && npm run compile

# Launch with F5 from VS Code
# - Should load config from test-workspace/graphql.config.yaml
# - Should validate example.graphql
# - Should validate embedded GraphQL in example.tsx
```
