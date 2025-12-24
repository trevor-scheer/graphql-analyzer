# GraphQL Tooling in Rust

A comprehensive GraphQL tooling ecosystem in Rust, providing LSP (Language Server Protocol) for editor integration and CLI for CI/CD enforcement.

## Project Structure

```
graphql-lsp/
├── crates/
│   ├── graphql-config/       # .graphqlrc parser and loader
│   ├── graphql-extract/      # Extract GraphQL from source files
│   ├── graphql-introspect/   # GraphQL introspection and SDL conversion
│   ├── graphql-linter/       # Linting engine with custom rules
│   ├── graphql-project/      # Core: validation, indexing, diagnostics
│   ├── graphql-lsp/          # LSP server implementation
│   └── graphql-cli/          # CLI tool for CI/CD
└── .claude/
    └── project-plan.md       # Comprehensive project plan
```

## Crates

### graphql-config

Parses and loads `.graphqlrc` configuration files with parity to the npm `graphql-config` package.

**Features:**

- YAML and JSON config formats
- Single and multi-project configurations
- Schema and document patterns
- Configuration discovery (walks up directory tree)

### graphql-extract

Extracts GraphQL queries, mutations, and fragments from source files.

**Supported:**

- Raw GraphQL files (`.graphql`, `.gql`, `.gqls`)
- TypeScript/JavaScript (via SWC) - Coming soon
- Template literals with `gql` tags
- Magic comments (`/* GraphQL */`)

### graphql-introspect

Fetches GraphQL schemas from remote endpoints via introspection and converts them to SDL.

**Features:**

- Standard GraphQL introspection query execution
- Type-safe deserialization of introspection responses
- Conversion from introspection JSON to SDL strings
- Support for all GraphQL schema types and directives
- Automatic filtering of built-in types and directives

### graphql-linter

Flexible linting engine with support for different linting contexts.

**Features:**

- Document-level lints (fast, real-time feedback)
- Project-wide lints (comprehensive analysis)
- Schema validation rules
- Configurable severity levels
- Tool-specific configuration (LSP vs CLI)

**Current rules:**

- `no_deprecated` - Warns when using @deprecated fields
- `unique_names` - Ensures operation/fragment names are unique
- `unused_fields` - Detects schema fields never used in operations (opt-in)

### graphql-project

Core library providing validation, indexing, and diagnostics.

**Features:**

- Schema loading from files and URLs
- Document loading and extraction
- Apollo compiler validation engine
- Schema and document indexing
- Diagnostic system
- Type information and hover support

### graphql-lsp

Language Server Protocol implementation for GraphQL.

**Implemented Features:**

- Real-time validation with project-wide diagnostics
- Configurable linting with custom rules
- Comprehensive go-to-definition support:
  - Fragment spreads, operations, types, fields
  - Variables, arguments, enum values
  - Directives and directive arguments
- Find references for fragments and type definitions
- Hover information for types and fields
- Works with embedded GraphQL in TypeScript/JavaScript

**Planned Features:**

- Additional find references support (fields, variables, directives, enum values)
- Autocomplete
- Document symbols
- Code actions

### graphql-cli

Command-line tool for validation, linting, and CI/CD integration.

**Commands:**

- `graphql validate` - Validate schema and documents (Apollo compiler validation)
- `graphql lint` - Run custom lint rules with configurable severity
- `graphql check` - Check for breaking changes (coming soon)

## Installation

### CLI Tool

#### Install from Binary (Recommended)

**macOS and Linux:**

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/trevor-scheer/graphql-lsp/releases/latest/download/graphql-cli-installer.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://github.com/trevor-scheer/graphql-lsp/releases/latest/download/graphql-cli-installer.ps1 | iex
```

#### Install from Source

```bash
cargo install --git https://github.com/trevor-scheer/graphql-lsp graphql-cli
```

#### Download Binary Directly

Download the appropriate binary for your platform from the [releases page](https://github.com/trevor-scheer/graphql-lsp/releases):

- macOS (Intel): `graphql-cli-x86_64-apple-darwin.tar.xz`
- macOS (Apple Silicon): `graphql-cli-aarch64-apple-darwin.tar.xz`
- Linux (x86_64): `graphql-cli-x86_64-unknown-linux-gnu.tar.xz`
- Linux (ARM64): `graphql-cli-aarch64-unknown-linux-gnu.tar.xz`
- Windows: `graphql-cli-x86_64-pc-windows-msvc.zip`

### LSP Server

The VSCode extension will automatically download and install the LSP server binary on first use. However, you can also install it manually:

#### Automatic Installation (Recommended)

Simply install the VSCode extension - it will download the appropriate binary for your platform automatically.

#### Manual Installation

**Via cargo:**

```bash
cargo install graphql-lsp
```

**From releases:**
Download the appropriate binary from the [releases page](https://github.com/trevor-scheer/graphql-lsp/releases):

- macOS (Intel): `graphql-lsp-x86_64-apple-darwin.tar.xz`
- macOS (Apple Silicon): `graphql-lsp-aarch64-apple-darwin.tar.xz`
- Linux (x86_64): `graphql-lsp-x86_64-unknown-linux-gnu.tar.xz`
- Linux (ARM64): `graphql-lsp-aarch64-unknown-linux-gnu.tar.xz`
- Windows: `graphql-lsp-x86_64-pc-windows-msvc.zip`

**Custom binary path:**
Set the `graphql-lsp.serverPath` setting in VSCode to point to a custom binary location.

**For development:**
The extension will automatically use `target/debug/graphql-lsp` when running from the repository, or you can set the `GRAPHQL_LSP_PATH` environment variable.

## Getting Started

### Using the CLI

#### Basic Usage

```bash
# Validate your GraphQL project (Apollo compiler validation)
graphql validate

# Run lints with configured rules
graphql lint

# Validate with a specific config file
graphql --config .graphqlrc.yml validate

# Output as JSON for CI/CD
graphql validate --format json
graphql lint --format json

# Watch mode for development (coming soon)
graphql validate --watch
graphql lint --watch
```

#### Multi-Project Configurations

When using a multi-project configuration, you must specify which project to use with the `--project` flag, unless your config includes a project named `default`.

**Single-project config** - No `--project` flag needed:
```yaml
# .graphqlrc.yml
schema: "schema.graphql"
documents: "src/**/*.graphql"
```
```bash
graphql validate
graphql lint
```

**Multi-project config** - Requires `--project` flag:
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
# Must specify which project to validate/lint
graphql validate --project frontend
graphql lint --project backend
```

**Multi-project with "default"** - Optional `--project` flag:
```yaml
# .graphqlrc.yml
projects:
  default:
    schema: "schema.graphql"
    documents: "src/**/*.graphql"
  experimental:
    schema: "experimental/schema.graphql"
    documents: "experimental/**/*.graphql"
```
```bash
# Uses "default" project automatically
graphql validate
graphql lint

# Or explicitly specify a project
graphql validate --project experimental
```

If you omit `--project` with a multi-project config (without "default"), you'll see an error listing available projects:

```
Error: Multi-project configuration requires --project flag

Available projects:
  - frontend
  - backend

Usage: graphql --project <NAME> validate
```

### Development

#### Build

```bash
cargo build --workspace
```

#### Run Tests

```bash
cargo test --workspace
```

#### Run CLI from Source

```bash
cargo run -p graphql-cli -- validate --help
```

#### Run LSP Server

```bash
cargo run -p graphql-lsp
```

## Development Status

✅ **Completed:**

- Cargo workspace structure
- graphql-config implementation (parsing, loading, validation)
- Core validation engine with project-wide diagnostics
- Document loading and indexing
- TypeScript/JavaScript extraction
- LSP features: validation, go-to-definition, find references, hover
- Schema and document indexing

🚧 **In Progress:**

- VS Code extension improvements
- Additional LSP features (completions, document symbols)

📋 **Planned:**

- Breaking change detection
- Code actions and refactoring
- Remote schema introspection
- Additional find references support (fields, variables, directives, enum values)

## Configuration

Configuration files support IDE validation and autocompletion via JSON Schema. See [graphqlrc.schema.md](./crates/graphql-config/schema/graphqlrc.schema.md) for setup instructions.

### Configuration Example

`.graphqlrc.yml`:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/trevor-scheer/graphql-lsp/main/crates/graphql-config/schema/graphqlrc.schema.json
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"
lint:
  # Enable recommended lints
  recommended: error
  rules:
    # Override specific rules
    no_deprecated: warn
    unique_names: off
```

Multi-project:

```yaml
projects:
  frontend:
    schema: "https://api.example.com/graphql"
    documents: "frontend/**/*.ts"
    lint:
      recommended: error
  backend:
    schema: "backend/schema.graphql"
    documents: "backend/**/*.graphql"
    lint:
      recommended: warn
```

### Lint Configuration

Linting is configured via top-level `lint` with optional tool-specific overrides:

**Available rules:**

- `no_deprecated` - Warns when using fields marked with @deprecated (recommended: warn)
- `unique_names` - Ensures operation and fragment names are unique (recommended: error)
- `unused_fields` - Detects schema fields never used (off by default, expensive)

**Severity levels:**

- `off` - Disable the rule
- `warn` - Show as warning
- `error` - Show as error

**Basic configuration:**

```yaml
# Top-level lint applies to all tools
lint:
  recommended: error  # Enable recommended rules
```

**Tool-specific overrides:**

```yaml
# Base configuration
lint:
  recommended: error
  rules:
    unused_fields: off  # Expensive, off by default

# Tool-specific overrides
extensions:
  # CLI: Enable expensive lints for CI
  cli:
    lint:
      rules:
        unused_fields: error

  # LSP: Keep expensive lints off for performance
  lsp:
    lint:
      rules:
        unused_fields: off
```

**Per-project configuration:**

```yaml
projects:
  default:
    schema: "schema.graphql"
    documents: "src/**/*.graphql"
    lint:
      recommended: error
      rules:
        no_deprecated: off  # Project-specific override
```

## License

MIT OR Apache-2.0
