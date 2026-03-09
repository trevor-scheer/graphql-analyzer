# graphql-lsp for Claude Code

A Claude Code plugin that provides GraphQL language intelligence via `graphql-lsp`. Get real-time validation, go-to-definition, find references, and hover information for `.graphql` files and embedded GraphQL in TypeScript/JavaScript.

## Install the plugin

### Add the marketplace

This plugin is distributed via the `graphql-analyzer` plugin marketplace on GitHub. Add it to your Claude Code settings:

```sh
/plugin marketplace add trevor-scheer/graphql-analyzer
```

Then install the plugin:

```sh
/plugin install graphql-lsp@graphql-analyzer
```

See the [Claude Code plugin documentation](https://docs.anthropic.com/en/docs/claude-code/plugins) for more details on managing plugins and marketplaces.

### Prerequisites

The plugin requires the `graphql-lsp` binary to be installed and available on your `PATH`.

## Install graphql-lsp

### From binary release (recommended)

Download the latest binary for your platform from the [releases page](https://github.com/trevor-scheer/graphql-analyzer/releases) (look for the `graphql-analyzer-lsp` release):

| Platform              | Asset                                       |
| --------------------- | ------------------------------------------- |
| macOS (Apple Silicon)  | `graphql-lsp-aarch64-apple-darwin.tar.xz`   |
| macOS (Intel)          | `graphql-lsp-x86_64-apple-darwin.tar.xz`    |
| Linux (x86_64)         | `graphql-lsp-x86_64-unknown-linux-gnu.tar.xz` |
| Linux (ARM64)          | `graphql-lsp-aarch64-unknown-linux-gnu.tar.xz` |
| Windows                | `graphql-lsp-x86_64-pc-windows-msvc.zip`    |

Extract the binary and place it somewhere on your `PATH`:

```sh
# macOS / Linux
tar -xJf graphql-lsp-*.tar.xz
mv graphql-lsp ~/.local/bin/
```

### Verify installation

```sh
graphql-lsp --version
```

## Configure your project

### 1. Add a `.graphqlrc.yaml`

Create a `.graphqlrc.yaml` (or `.graphqlrc.yml`, `.graphqlrc.json`) in your project root:

```yaml
# Single project
schema: schema.graphql
documents: "src/**/*.{graphql,ts,tsx}"
```

For multi-project workspaces:

```yaml
projects:
  web:
    schema: packages/web/schema.graphql
    documents: "packages/web/src/**/*.{graphql,tsx}"
  api:
    schema: packages/api/schema.graphql
    documents: "packages/api/src/**/*.graphql"
```

Remote schemas are also supported:

```yaml
schema: https://api.example.com/graphql
documents: "src/**/*.{graphql,ts,tsx}"
```

### 2. Configure linting (optional)

```yaml
schema: schema.graphql
documents: "src/**/*.{graphql,ts,tsx}"
extensions:
  lint:
    extends: recommended
    rules:
      noDeprecated: warn
      uniqueNames: error
```

## What you get

Once installed and configured, Claude Code will use the GraphQL LSP to provide:

- **Diagnostics** - Real-time validation errors and lint warnings
- **Go to definition** - Navigate to fragment definitions, type definitions, field definitions, and more
- **Find references** - Find all usages of fragments and types across the project
- **Hover** - Type information, descriptions, and deprecation warnings
- **Embedded GraphQL** - Full support for `gql` tagged template literals in TypeScript/JavaScript

## Troubleshooting

**LSP not starting?** Make sure `graphql-lsp` is on your `PATH` and that a `.graphqlrc.yaml` exists at your project root.

**No diagnostics?** Check that your `documents` glob pattern matches your file locations.

**Embedded GraphQL not detected?** The LSP recognizes `gql` and `graphql` tag functions from known modules like `graphql-tag` and `@apollo/client`. Add custom modules via:

```yaml
extensions:
  extractConfig:
    modules: ["graphql-tag", "your-custom-module"]
```
