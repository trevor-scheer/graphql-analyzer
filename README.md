# GraphQL Tooling in Rust

A comprehensive GraphQL tooling ecosystem in Rust, providing LSP (Language Server Protocol) for editor integration and CLI for CI/CD enforcement.

## Project Structure

```
graphql-lsp/
├── crates/
│   ├── graphql-config/       # .graphqlrc parser and loader
│   ├── graphql-extract/      # Extract GraphQL from source files
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

### graphql-project
Core library providing validation, indexing, and diagnostics.

**Features:**
- Schema loading from files and URLs
- Document loading and extraction
- Validation engine
- Schema and document indexing
- Diagnostic system

### graphql-lsp
Language Server Protocol implementation for GraphQL.

**Features:**
- ✅ Real-time validation with accurate diagnostics
- ✅ Go-to-definition (fragments, types, fields, variables, directives, enum values, arguments)
- ✅ Find references (fragments, type definitions)
- ✅ Hover information with type details
- 🚧 Autocomplete (in progress)
- 🚧 Document symbols (in progress)

### graphql-cli
Command-line tool for validation and CI/CD integration.

**Commands:**
- `graphql validate` - Validate schema and documents
- `graphql check` - Check for breaking changes (coming soon)

## Getting Started

### Build

```bash
cargo build --workspace
```

### Run Tests

```bash
cargo test --workspace
```

### Run CLI

```bash
cargo run -p graphql-cli -- validate --help
```

### Run LSP Server

```bash
cargo run -p graphql-lsp
```

## Development Status

✅ **Completed:**
- Cargo workspace structure
- graphql-config implementation (parsing, loading, validation)
- Core validation engine with accurate diagnostics
- Document loading and indexing
- TypeScript/JavaScript extraction
- LSP goto definition (comprehensive support for all GraphQL constructs)
- LSP find references (fragments and type definitions)
- LSP hover information
- VS Code extension

🚧 **In Progress:**
- LSP autocomplete
- LSP document symbols

📋 **Planned:**
- Breaking change detection
- Remote schema introspection
- Additional find references support (fields, variables, directives, enum values)

## Configuration Example

`.graphqlrc.yml`:
```yaml
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"
```

Multi-project:
```yaml
projects:
  frontend:
    schema: "https://api.example.com/graphql"
    documents: "frontend/**/*.ts"
  backend:
    schema: "backend/schema.graphql"
    documents: "backend/**/*.graphql"
```

## License

MIT OR Apache-2.0
