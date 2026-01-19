# GraphQL Tooling in Rust

A comprehensive GraphQL tooling ecosystem in Rust, providing LSP (Language Server Protocol) for editor integration and CLI for CI/CD enforcement.

## Project Structure

```
graphql-lsp/
├── crates/
│   ├── graphql-config/       # .graphqlrc parser and loader
│   ├── graphql-extract/      # Extract GraphQL from source files
│   ├── graphql-introspect/   # GraphQL introspection and SDL conversion
│   ├── graphql-db/           # Salsa database and input queries
│   ├── graphql-syntax/       # GraphQL parsing and syntax trees
│   ├── graphql-hir/          # High-level semantic representation
│   ├── graphql-analysis/     # Query-based validation and analysis
│   ├── graphql-linter/       # Linting engine with custom rules
│   ├── graphql-ide/          # Editor-facing IDE features API
│   ├── graphql-lsp/          # LSP server implementation
│   ├── graphql-mcp/          # MCP server for AI agents
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

### graphql-ide

Editor-facing API layer providing IDE features through the Salsa-based analysis infrastructure.

**Features:**

- Schema loading from config (with Apollo Client built-in directives)
- Document management and change tracking
- Real-time validation and diagnostics
- Linting with configurable rules
- Type information and hover support
- Go-to-definition and find references
- Completion suggestions

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

### graphql-mcp

MCP (Model Context Protocol) server for AI agent integration.

**Features:**

- Schema-aware validation
- Linting with diagnostics
- Multi-project support
- On-demand project loading

**Available Tools:**

- `validate_document` - Validate GraphQL against schema
- `lint_document` - Run lint rules on a document
- `list_projects` - List available projects
- `load_project` - Load a project on demand
- `get_project_diagnostics` - Get diagnostics for all files

See [graphql-mcp README](crates/graphql-mcp/README.md) for setup instructions.

### graphql-cli

Command-line tool for validation, linting, and CI/CD integration.

**Commands:**

- `graphql validate` - Validate schema and documents (Apollo compiler validation)
- `graphql lint` - Run custom lint rules with configurable severity
- `graphql mcp` - Start MCP server for AI agent integration
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

#### Run Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench parse_cold

# Compare against baseline
cargo bench -- --save-baseline main
cargo bench -- --baseline main
```

See [benches/README.md](benches/README.md) for detailed information about the benchmark suite and interpreting results.

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
- TypeScript/JavaScript extraction and position mapping
- Remote schema introspection via URL
- Comprehensive linting system with multiple rule types
- LSP features:
  - Real-time validation and diagnostics
  - Comprehensive goto definition (fragments, types, fields, variables, arguments, enum values, directives)
  - Find references (fragments, types, fields)
  - Hover information
- CLI tools (validate, lint) with JSON output
- MCP server for AI agent integration
- VSCode extension with automatic LSP binary download

🚧 **In Progress:**

- Query-based architecture migration (Salsa)
- Additional IDE features (completions, document symbols)

📋 **Planned:**

- Breaking change detection
- Code actions and refactoring
- Semantic highlighting
- Additional find references (variables, directives, enum values)

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

| Rule | Description | Recommended |
|------|-------------|-------------|
| `unique_names` | Ensures operation and fragment names are unique across the project | error |
| `no_anonymous_operations` | Requires all operations to have explicit names for better monitoring and debugging | error |
| `no_deprecated` | Warns when using deprecated fields, arguments, or enum values | warn |
| `redundant_fields` | Detects fields that are redundant because they are already included in a sibling fragment spread | warn |
| `require_id_field` | Warns when the `id` field is not requested on types that have it | warn |
| `unused_fields` | Detects schema fields that are never used in any operation or fragment | - |
| `unused_fragments` | Detects fragment definitions that are never used in any operation | - |
| `unused_variables` | Detects variables declared in operations that are never used | - |
| `operation_name_suffix` | Requires operation names to have type-specific suffixes (Query, Mutation, Subscription) | - |

Rules marked with `-` in the Recommended column are not included in the `recommended` preset and must be explicitly enabled.

**Severity levels:**

- `off` - Disable the rule
- `warn` - Show as warning
- `error` - Show as error

**Using the recommended preset:**

```yaml
lint:
  # Enable all recommended rules at their predefined severities
  # (see Recommended column in the table above)
  recommended: error
```

Note: The value (`error` or `warn`) after `recommended:` enables the preset. Each rule in the preset runs at its own predefined severity level as shown in the table above.

**Enabling additional rules:**

```yaml
lint:
  recommended: error
  rules:
    # Add rules not in the recommended preset
    unused_fields: warn
    operation_name_suffix: error
```

**Overriding recommended rule severities:**

```yaml
lint:
  recommended: error
  rules:
    # Override a recommended rule's severity
    no_deprecated: off        # Disable entirely
    require_id_field: error   # Upgrade from warn to error
```

**Tool-specific overrides:**

```yaml
lint:
  recommended: error

extensions:
  cli:
    lint:
      rules:
        unused_fields: error   # Enable for CLI

  lsp:
    lint:
      rules:
        unused_fields: off     # Disable for LSP
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
        no_deprecated: off     # Project-specific override
```

## License

MIT OR Apache-2.0
