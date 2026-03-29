# GraphQL Analyzer

A fast, Rust-powered GraphQL tooling suite with IDE support, CLI validation, and AI agent integration.

GraphQL Analyzer provides real-time validation, navigation, and linting for GraphQL projects. It works with pure `.graphql` files and embedded GraphQL in TypeScript/JavaScript, with full support for multi-project workspaces and remote schema introspection.

Under the hood, the analyzer uses a query-based architecture with incremental computation via [Salsa](https://github.com/salsa-rs/salsa). This means only the parts of your project affected by a change are recomputed, keeping the IDE responsive even in large codebases.

This project draws heavy inspiration from [rust-analyzer](https://rust-analyzer.github.io/) for its architecture, and builds on the patterns established by [graphql-language-service](https://github.com/graphql/graphiql/tree/main/packages/graphql-language-service) and [graphql-config](https://github.com/kamilkisiela/graphql-config) from the GraphQL community.

## Quick Start

### 1. Install the VS Code Extension

Install **[GraphQL Analyzer](https://marketplace.visualstudio.com/items?itemName=trevor-scheer.graphql-analyzer)** from the VS Code Marketplace, or search "GraphQL Analyzer" in the Extensions view.

### 2. Configure Your Project

Create a config file in your project root (`.graphqlrc.yml`, `.graphqlrc.toml`, or `.graphqlrc.json`):

```yaml
# .graphqlrc.yml
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"
```

### 3. Start Coding

Open any GraphQL file or TypeScript/JavaScript file with embedded GraphQL. You'll get:

- **Real-time validation** - Instant feedback on errors
- **Go to definition** - Jump to types, fragments, and fields
- **Find references** - See all usages across your project
- **Hover information** - Type details and descriptions

## CLI Usage

Install the CLI for CI/CD integration:

```sh
# Install the CLI (default)
curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.sh | sh

# Install LSP or MCP server
curl -fsSL .../install.sh | sh -s -- lsp
curl -fsSL .../install.sh | sh -s -- mcp
```

Validate your GraphQL:

```bash
graphql validate
graphql lint
```

Use `--format json` or `--format github` for CI integration.

For full CLI documentation, see the **[CLI README](crates/cli/README.md)**.

## Configuration

```yaml
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"

extensions:
  lint:
    extends: recommended
    rules:
      noDeprecated: warn
```

For multi-project setups and advanced configuration, see the **[Configuration Guide](crates/config/README.md)**.

## Documentation

| Component                                                          | Description                                 |
| ------------------------------------------------------------------ | ------------------------------------------- |
| [VS Code Extension](editors/vscode/README.md)                      | IDE features, installation, troubleshooting |
| [CLI](crates/cli/README.md)                                        | Commands, CI/CD integration, output formats |
| [LSP Server](crates/lsp/README.md)                                 | Editor integration, Neovim setup, debugging |
| [MCP Server](crates/mcp/README.md)                                 | AI agent integration for Claude and others  |
| [Claude Code Plugin](.claude-plugin/plugins/graphql-lsp/README.md) | GraphQL LSP plugin for Claude Code          |
| [Development Guide](DEVELOPMENT.md)                                | Building from source, testing, contributing |

## License

MIT OR Apache-2.0
