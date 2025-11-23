# VS Code Extension Setup

This guide explains how to set up and test the GraphQL LSP VS Code extension.

## Quick Start

### 1. Build the LSP Server

```bash
cargo build --package graphql-lsp
```

The binary will be at `target/debug/graphql-lsp`

### 2. Set Up the Extension

```bash
cd editors/vscode
npm install
npm run compile
```

### 3. Launch the Extension

Open the `editors/vscode` directory in VS Code and press `F5` to launch the Extension Development Host.

### 4. Test with Example Files

In the Extension Development Host window:
1. Open the `test-workspace` folder
2. Open `example.graphql`
3. You should see validation errors for invalid fields

## Architecture

```
┌─────────────────┐
│   VS Code       │
│   Extension     │
│  (TypeScript)   │
└────────┬────────┘
         │ JSON-RPC over stdio
         │
┌────────▼────────┐
│   LSP Server    │
│     (Rust)      │
├─────────────────┤
│ • Validation    │
│ • Diagnostics   │
│ • (Future: more)│
└─────────────────┘
```

## Current Features

- ✅ Real-time GraphQL validation
- ✅ Error diagnostics with messages
- ✅ Support for `.graphql` and `.gql` files

## Future Features

- [ ] Extract line/column info from diagnostics for precise error locations
- [ ] Load schema from `graphql.config.yaml`
- [ ] Support embedded GraphQL in TypeScript/JavaScript
- [ ] Autocompletion
- [ ] Hover documentation
- [ ] Go-to-definition
- [ ] Find references

## Troubleshooting

### Extension not activating

Check that the file has a `.graphql` or `.gql` extension.

### LSP server not found

Set the `GRAPHQL_LSP_PATH` environment variable:
```bash
export GRAPHQL_LSP_PATH=/Users/trevor/Repositories/graphql-lsp/target/debug/graphql-lsp
```

Or modify the launch configuration in `.vscode/launch.json`.

### Enable verbose logging

1. In VS Code settings, set `graphql-lsp.trace.server` to `verbose`
2. Check the "Output" panel and select "GraphQL Language Server" from the dropdown

## Development

### Rebuilding the LSP Server

```bash
cargo build --package graphql-lsp
```

### Recompiling the Extension

```bash
cd editors/vscode
npm run compile
```

Then reload the Extension Development Host window (Ctrl+R / Cmd+R).

### Adding New LSP Features

1. Implement the feature in `crates/graphql-lsp/src/server.rs`
2. Rebuild the LSP server
3. Test in the Extension Development Host
