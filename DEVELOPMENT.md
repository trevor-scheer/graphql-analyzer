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

This project uses [cargo-nextest](https://nexte.st/) for running tests. It runs
test binaries in parallel and provides better output than `cargo test`.

```bash
# Install nextest
cargo install cargo-nextest

# Run all tests
cargo nextest run --workspace

# Run tests for a specific crate
cargo nextest run --package graphql-linter

# Run a specific test by name
cargo nextest run --workspace test_name

# Run with output (stdout visible)
cargo nextest run --workspace --no-capture
```

> **Note:** nextest doesn't support doctests. Run `cargo test --workspace --doc`
> separately if needed.

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
3. The extension automatically uses `target/debug/graphql-lsp` when running from the repo

### Commands

```bash
npm run compile      # Build extension
npm run watch        # Watch mode
npm run format       # Format TypeScript
npm run lint         # Lint TypeScript
npm run package      # Create .vsix file
```

### Testing Extension Builds

To test a platform-specific extension build from a PR, comment `/build-extension` on the PR. This will:

- Build LSP binaries for all platforms
- Package platform-specific VSIXs
- Post a comment with download links

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

graphql-analyzer supports [OpenTelemetry](https://opentelemetry.io/) tracing
for diagnosing performance issues. Traces provide detailed timing data for LSP
operations like file changes, validation, and schema loading.

#### VS Code

1. Start a collector (see [Running Jaeger](#running-jaeger) below)
2. Enable OTEL in VS Code settings:
   - Set `graphql-analyzer.debug.otelEnabled` to `true`
   - Optionally adjust `graphql-analyzer.debug.otelEndpoint` (default: `http://localhost:4317`)
3. Restart the language server (Command Palette: "graphql-analyzer: Restart Language Server")
4. Use the "graphql-analyzer: Test OpenTelemetry Connection" command to verify connectivity
5. Open [http://localhost:16686](http://localhost:16686) to view traces

#### VS Code Tracing Settings

| Setting              | Type    | Default                 | Description                                                                    |
| -------------------- | ------- | ----------------------- | ------------------------------------------------------------------------------ |
| `debug.logLevel`     | string  | `warn`                  | Server log verbosity. Higher levels may impact performance on large codebases. |
| `debug.otelEnabled`  | boolean | `false`                 | Export traces via OpenTelemetry to an OTLP collector.                          |
| `debug.otelEndpoint` | string  | `http://localhost:4317` | OTLP collector gRPC endpoint.                                                  |

All settings are under the `graphql-analyzer` namespace.

#### CLI

```bash
# Run with tracing enabled
OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp

# Custom endpoint
OTEL_TRACES_ENABLED=1 \
  OTEL_EXPORTER_OTLP_ENDPOINT=http://my-collector:4317 \
  target/debug/graphql-lsp

# Combine with log level control
RUST_LOG=info OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp
```

#### Running Jaeger

[Jaeger](https://www.jaegertracing.io/) is an open-source tracing backend that
works out of the box with graphql-analyzer's OTLP export.

**Docker:**

```bash
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

**Podman:**

```bash
podman run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  docker.io/jaegertracing/all-in-one:latest
```

**Docker Compose:**

```yaml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "4317:4317" # OTLP gRPC
      - "16686:16686" # Jaeger UI
```

| Port  | Purpose                                                 |
| ----- | ------------------------------------------------------- |
| 4317  | OTLP gRPC ingestion (what graphql-analyzer connects to) |
| 16686 | Jaeger UI for viewing traces                            |

To stop Jaeger: `docker stop jaeger && docker rm jaeger` (or `podman`).

#### Log Levels and Performance

The default log level is `warn`, which has negligible overhead even on large
codebases (10k+ files). Setting the level to `info` or `debug` activates text
log formatting for all instrumented functions, which can noticeably impact
performance during initial load and file changes.

For performance investigation, use OTEL tracing (which batches and exports
asynchronously) rather than increasing the log level. To debug a specific
module without global overhead:

```bash
RUST_LOG=warn,graphql_lsp::server=debug target/debug/graphql-lsp
```

#### Interpreting Traces

In Jaeger UI, look for the `graphql-analyzer` service. Key spans:

- **`did_change`** -- Triggered on every edit. Shows time in change application and re-validation.
- **`did_save`** -- Triggered on file save. Includes cross-file diagnostics.
- **`validate_file_with_snapshot`** -- Core validation path (parsing, HIR, analysis).
- **`load_workspace_config`** -- Initial project loading (config, file discovery, introspection).

#### Troubleshooting

- **No traces appearing:** Run "graphql-analyzer: Test OpenTelemetry Connection" to verify the collector is reachable. Check the "graphql-analyzer Debug" output channel for OTEL messages. Ensure the server was restarted after enabling OTEL.
- **Collector not reachable:** Verify the container is running (`docker ps`). Check port 4317 is free.
- **High overhead:** Check if `debug.logLevel` is `info` or `debug` -- text log formatting is typically the bottleneck, not OTEL.

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
