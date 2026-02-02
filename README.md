# GraphQL Analyzer

A comprehensive GraphQL tooling ecosystem in Rust, providing IDE features via LSP and CLI tools for CI/CD.

## Installation

### CLI

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.ps1 | iex
```

Or download directly from the [releases page](https://github.com/trevor-scheer/graphql-analyzer/releases).

### VSCode Extension

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install-vscode.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install-vscode.ps1 | iex
```

Or download the `.vsix` from the [releases page](https://github.com/trevor-scheer/graphql-analyzer/releases) and install with `code --install-extension <file>.vsix`.

## Quick Start

1. Create a `.graphqlrc.yml` in your project root:

```yaml
schema: "schema.graphql"
documents: "src/**/*.graphql"
```

2. Run validation:

```bash
graphql validate
```

3. Run linting:

```bash
graphql lint
```

## CLI Commands

| Command            | Description                        |
| ------------------ | ---------------------------------- |
| `graphql validate` | Validate GraphQL documents         |
| `graphql lint`     | Lint with configurable rules       |
| `graphql check`    | Run both validate and lint         |
| `graphql lsp`      | Start the language server          |
| `graphql mcp`      | Start the MCP server for AI agents |

Use `--format json` for CI/CD integration.

## Configuration

`.graphqlrc.yml`:

```yaml
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"
lint:
  recommended: error
  rules:
    no_deprecated: warn
```

### Multi-Project

```yaml
projects:
  frontend:
    schema: "https://api.example.com/graphql"
    documents: "frontend/**/*.ts"
  backend:
    schema: "backend/schema.graphql"
    documents: "backend/**/*.graphql"
```

Use `--project <name>` to specify which project to validate/lint.

### Lint Rules

| Rule                      | Description                                | Default |
| ------------------------- | ------------------------------------------ | ------- |
| `unique_names`            | Unique operation/fragment names            | error   |
| `no_anonymous_operations` | Require named operations                   | error   |
| `no_deprecated`           | Warn on deprecated usage                   | warn    |
| `redundant_fields`        | Detect fields already in fragment spreads  | warn    |
| `require_id_field`        | Warn when `id` field not requested         | warn    |
| `unused_fields`           | Detect unused schema fields                | off     |
| `unused_fragments`        | Detect unused fragments                    | off     |
| `unused_variables`        | Detect unused variables                    | off     |
| `operation_name_suffix`   | Require Query/Mutation/Subscription suffix | off     |

Enable the recommended preset with `recommended: error`, then override individual rules as needed.

## IDE Features

The language server provides:

- Real-time validation and diagnostics
- Go-to-definition (types, fields, fragments, variables, directives)
- Find references
- Hover information
- Works with embedded GraphQL in TypeScript/JavaScript

## MCP Server

For AI agent integration, the MCP server exposes GraphQL tooling:

```bash
graphql mcp
```

See [graphql-mcp README](crates/mcp/README.md) for setup instructions.

---

## Development

### Building from Source

```bash
cargo build --workspace
cargo test --workspace
```

### Install from Source

```bash
cargo install --git https://github.com/trevor-scheer/graphql-analyzer graphql-cli
```

### Project Structure

```
graphql-analyzer/
├── crates/
│   ├── config/       # .graphqlrc parser
│   ├── extract/      # Extract GraphQL from TS/JS
│   ├── introspect/   # Remote schema introspection
│   ├── base-db/      # Salsa database
│   ├── syntax/       # Parsing
│   ├── hir/          # Semantic representation
│   ├── analysis/     # Validation
│   ├── linter/       # Lint rules
│   ├── ide/          # IDE features API
│   ├── lsp/          # LSP server
│   ├── mcp/          # MCP server
│   └── cli/          # CLI
└── editors/
    └── vscode/       # VSCode extension
```

## License

MIT OR Apache-2.0
