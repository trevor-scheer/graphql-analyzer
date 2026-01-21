# GraphQL Tooling in Rust

A comprehensive GraphQL tooling ecosystem in Rust, providing a unified CLI with LSP (Language Server Protocol) for editor integration, MCP (Model Context Protocol) for AI agents, and validation/linting for CI/CD enforcement.

## Features

- **Unified CLI** - Single `graphql` binary for validation, linting, analysis, LSP, and MCP
- **LSP Server** - Real-time diagnostics, goto definition, find references, hover, and completions
- **MCP Server** - AI agent integration via Model Context Protocol
- **Incremental Architecture** - Query-based computation using Salsa for fast, incremental analysis
- **Embedded GraphQL** - Works with `.graphql` files and embedded GraphQL in TypeScript/JavaScript
- **Remote Schema Support** - Introspect and validate against remote GraphQL endpoints

## Quick Start

### Installation

**macOS and Linux:**

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/trevor-scheer/graphql-lsp/releases/latest/download/graphql-cli-installer.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://github.com/trevor-scheer/graphql-lsp/releases/latest/download/graphql-cli-installer.ps1 | iex
```

**Install from source:**

```bash
cargo install --git https://github.com/trevor-scheer/graphql-lsp graphql-cli
```

### Basic Usage

```bash
# Validate GraphQL schema and documents
graphql validate

# Run lint rules
graphql lint

# Run both validation and linting
graphql check

# Start the LSP server (for editor integration)
graphql lsp

# Start the MCP server (for AI agents)
graphql mcp
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `validate` | Validate GraphQL schema and documents against the GraphQL spec |
| `lint` | Run custom lint rules on GraphQL documents |
| `check` | Run both validation and linting in a single pass (recommended for CI) |
| `deprecations` | List all deprecated field usages across the project |
| `schema download` | Download schema from a remote GraphQL endpoint via introspection |
| `stats` | Display statistics about the GraphQL project |
| `fragments` | Analyze fragment usage across the project |
| `coverage` | Show schema field coverage by operations |
| `complexity` | Analyze query complexity for GraphQL operations |
| `lsp` | Start the Language Server Protocol server |
| `mcp` | Start the Model Context Protocol server for AI agents |

### Common Options

```bash
graphql --config .graphqlrc.yml validate   # Use specific config file
graphql --project frontend lint            # Target specific project
graphql validate --format json             # JSON output for CI/CD
graphql lint --fix                         # Auto-fix lint issues
graphql check --watch                      # Watch mode for development
```

### Multi-Project Configurations

When using a multi-project configuration, specify which project to use with `--project`:

```yaml
# .graphqlrc.yml
projects:
  frontend:
    schema: "frontend/schema.graphql"
    documents: "frontend/**/*.ts"
  backend:
    schema: "backend/schema.graphql"
    documents: "backend/**/*.graphql"
```

```bash
graphql validate --project frontend
graphql lint --project backend
```

If your config includes a project named `default`, the `--project` flag becomes optional.

## Editor Integration

### VSCode

Install the GraphQL LSP extension from the VSCode marketplace. The extension automatically downloads the appropriate binary for your platform.

**Manual setup:** Set `graphql.server.path` to a custom binary location, or the extension will search PATH and download automatically.

**For development:** The extension uses `target/debug/graphql` when running from the repository.

### Other Editors

Configure your editor's LSP client to run:

```bash
graphql lsp
```

The LSP server communicates via stdio using JSON-RPC.

## LSP Features

- **Real-time validation** - Immediate feedback as you type
- **Diagnostics** - Validation errors and lint warnings
- **Goto definition** - Navigate to types, fields, fragments, variables, arguments, enum values, and directives
- **Find references** - Find all usages of types, fields, and fragments
- **Hover** - Type information and descriptions
- **Completions** - Suggestions for fields, types, arguments, and more
- **Semantic highlighting** - Syntax highlighting with deprecated field strikethrough
- **Remote schema support** - Navigate into introspected schemas

## MCP Server

The MCP server exposes GraphQL tooling to AI agents via the Model Context Protocol:

```bash
graphql mcp                           # Start with all projects loaded
graphql mcp --no-preload              # Lazy load projects on demand
graphql mcp --preload frontend,api    # Only preload specific projects
```

**Available Tools:**

- `validate_document` - Validate GraphQL against schema
- `lint_document` - Run lint rules on a document
- `list_projects` - List available projects
- `load_project` - Load a project on demand
- `get_project_diagnostics` - Get diagnostics for all files

See [graphql-mcp README](crates/graphql-mcp/README.md) for setup instructions.

## Configuration

Create a `.graphqlrc.yml` (or `.graphqlrc.yaml`, `.graphqlrc.json`) in your project root:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/trevor-scheer/graphql-lsp/main/crates/graphql-config/schema/graphqlrc.schema.json
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"
lint:
  extends: recommended
  rules:
    no_deprecated: warn
```

### Remote Schema

```yaml
schema: "https://api.example.com/graphql"
documents: "src/**/*.graphql"
```

### Lint Rules

| Rule | Description | Recommended |
|------|-------------|-------------|
| `unique_names` | Ensures operation and fragment names are unique across the project | error |
| `no_anonymous_operations` | Requires all operations to have explicit names | error |
| `no_deprecated` | Warns when using deprecated fields, arguments, or enum values | warn |
| `redundant_fields` | Detects fields already included via fragment spreads | warn |
| `require_id_field` | Warns when `id` field is not requested on types that have it | warn |
| `unused_fields` | Detects schema fields never used in any operation | - |
| `unused_fragments` | Detects fragment definitions never used | - |
| `unused_variables` | Detects variables declared but never used | - |
| `operation_name_suffix` | Requires operation names to have type-specific suffixes | - |

Rules marked `-` are not in the recommended preset and must be explicitly enabled.

**Using presets:**

```yaml
lint:
  extends: recommended  # Enable recommended rules at their default severities
  rules:
    unused_fields: warn  # Add non-recommended rules
    no_deprecated: off   # Override recommended rule severity
```

**Tool-specific overrides:**

```yaml
lint:
  extends: recommended

extensions:
  cli:
    lint:
      rules:
        unused_fields: error  # Strict in CI
  lsp:
    lint:
      rules:
        unused_fields: off    # Relaxed in editor
```

## Project Structure

```
graphql-lsp/
├── crates/
│   ├── graphql-cli/          # Unified CLI (validate, lint, lsp, mcp, etc.)
│   ├── graphql-lsp/          # LSP server implementation
│   ├── graphql-mcp/          # MCP server for AI agents
│   ├── graphql-ide/          # Editor-facing IDE features API
│   ├── graphql-analysis/     # Query-based validation and analysis
│   ├── graphql-linter/       # Linting engine with custom rules
│   ├── graphql-hir/          # High-level semantic representation
│   ├── graphql-syntax/       # GraphQL parsing and syntax trees
│   ├── graphql-base-db/      # Salsa database foundation
│   ├── graphql-ide-db/       # Full Salsa database for IDE
│   ├── graphql-config/       # .graphqlrc parser and loader
│   ├── graphql-extract/      # Extract GraphQL from TS/JS files
│   ├── graphql-introspect/   # Remote schema introspection
│   ├── graphql-apollo-ext/   # Apollo parser extensions and utilities
│   └── graphql-test-utils/   # Shared test utilities
└── editors/
    └── vscode/               # VSCode extension
```

## Development

### Build

```bash
cargo build                    # Debug build
cargo build --release          # Release build
```

### Test

```bash
cargo test                     # Run all tests
cargo test --package graphql-linter  # Test specific crate
```

### Benchmarks

```bash
cargo bench                    # Run all benchmarks
cargo bench parse_cold         # Run specific benchmark
cargo bench -- --save-baseline main  # Save baseline for comparison
```

See [benches/README.md](benches/README.md) for benchmark details.

### Run from Source

```bash
cargo run -p graphql-cli -- validate
cargo run -p graphql-cli -- lsp
cargo run -p graphql-cli -- --help
```

## Architecture

The project uses a query-based, incremental architecture inspired by [rust-analyzer](https://rust-analyzer.github.io/book/contributing/architecture.html):

```
graphql-cli         ← Unified CLI entry point
    ↓
graphql-lsp/mcp     ← Protocol adapters (LSP, MCP)
    ↓
graphql-ide         ← Editor API with POD types
    ↓
graphql-analysis    ← Validation and linting
    ↓
graphql-hir         ← High-level IR (semantic queries)
    ↓
graphql-syntax      ← Parsing (file-local, cached)
    ↓
graphql-base-db     ← Salsa database (inputs, memoization)
```

Key benefits:
- **Automatic memoization** - Query results cached by inputs
- **Incremental invalidation** - Only affected queries re-run on changes
- **Fine-grained caching** - Per-file granular invalidation
- **Lazy evaluation** - Queries only run when results are needed

## License

MIT OR Apache-2.0
