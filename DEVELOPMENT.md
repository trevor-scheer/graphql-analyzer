# Development Guide

This guide covers building, testing, and contributing to GraphQL Analyzer.

## Prerequisites

- Rust toolchain (see `rust-toolchain.toml` for version)
- Node.js and npm (for VS Code extension)
- Cargo (included with Rust)

## Building from Source

### Build Everything

```bash
cargo build --workspace
```

### Build Specific Components

```bash
# CLI
cargo build --package graphql-cli

# LSP server
cargo build --package graphql-lsp

# Release builds
cargo build --workspace --release
```

### Install CLI from Source

```bash
cargo install --git https://github.com/trevor-scheer/graphql-analyzer graphql-cli
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test --package graphql-linter

# Run with output
cargo test -- --nocapture
```

## Linting and Formatting

```bash
# Format code
cargo fmt

# Lint with Clippy
cargo clippy --workspace
```

## VS Code Extension Development

### Setup

```bash
cd editors/vscode
npm install
npm run compile
```

### Development Mode

1. Open `editors/vscode` in VS Code
2. Press `F5` to launch the Extension Development Host
3. The extension automatically uses `target/debug/graphql` when running from the repo

### Commands

```bash
npm run compile      # Build extension
npm run watch        # Watch mode
npm run format       # Format TypeScript
npm run lint         # Lint TypeScript
npm run package      # Create .vsix file
```

## Benchmarking

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench parse_cold

# Save baseline for comparison
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main
```

## Debugging

### LSP Server

```bash
# Run with debug logging
RUST_LOG=debug target/debug/graphql-lsp

# Module-specific logging
RUST_LOG=graphql_lsp=debug,graphql_analysis=info target/debug/graphql-lsp
```

### OpenTelemetry Tracing

```bash
# Build with tracing support
cargo build --features otel

# Start Jaeger
docker run -d --name jaeger -p 4317:4317 -p 16686:16686 jaegertracing/all-in-one:latest

# Run with tracing enabled
OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp

# View traces at http://localhost:16686
```

## Project Structure

```
graphql-analyzer/
├── crates/
│   ├── analysis/     # Validation layer (Salsa queries)
│   ├── base-db/      # Salsa database foundation
│   ├── cli/          # CLI tool
│   ├── config/       # .graphqlrc parser
│   ├── extract/      # Extract GraphQL from TS/JS
│   ├── hir/          # High-level IR (semantic layer)
│   ├── ide/          # IDE features API
│   ├── ide-db/       # IDE database extensions
│   ├── introspect/   # Remote schema introspection
│   ├── linter/       # Lint rules engine
│   ├── lsp/          # LSP server
│   ├── mcp/          # MCP server
│   └── syntax/       # Parsing layer
├── editors/
│   └── vscode/       # VS Code extension
├── benches/          # Performance benchmarks
└── tests/            # Integration tests
```

## Architecture

The codebase uses a query-based, incremental architecture inspired by [rust-analyzer](https://rust-analyzer.github.io/book/contributing/architecture.html):

```
graphql-lsp / graphql-cli / graphql-mcp
    ↓
graphql-ide (Editor API)
    ↓
graphql-analysis (Validation + Linting)
    ↓
graphql-hir (Semantic layer)
    ↓
graphql-syntax (Parsing)
    ↓
graphql-db (Salsa database)
```

Key technologies:
- **Salsa** - Incremental computation framework
- **tower-lsp** - LSP framework
- **apollo-compiler** - GraphQL parsing and validation

## Creating Releases

Releases are automated via CI using [Knope](https://knope.tech) with changesets.

### Create a Changeset

```bash
# Interactive mode
knope document-change

# Or create manually in .changeset/
```

Changeset format:

```markdown
---
graphql-lsp: minor
---

Add support for feature X
```

### Release Flow

1. Create changesets for your changes
2. Merge to main
3. CI creates a Release PR
4. Merge the Release PR
5. CI builds and publishes releases

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `cargo fmt` and `cargo clippy`
5. Run `cargo test`
6. Open a pull request

For VS Code extension changes, also run:

```bash
cd editors/vscode
npm run format:check
npm run lint
```
