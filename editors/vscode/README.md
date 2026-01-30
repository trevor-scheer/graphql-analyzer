# GraphQL LSP VS Code Extension

A VS Code extension that provides comprehensive GraphQL language support including validation, navigation, hover information, and find references.

## Features

### Validation & Diagnostics

- **Real-time validation**: Instant feedback on GraphQL syntax and semantic errors
- **Schema validation**: Type checking against your GraphQL schema
- **Configurable linting**: Custom lint rules with tool-specific configuration
- **Project-wide analysis**: Validates fragment usage across files

### Navigation

- **Goto Definition**: Navigate to definitions of:
  - Fragment spreads → Fragment definitions
  - Type references → Type definitions
  - Field selections → Schema field definitions
  - Variables → Variable definitions
  - Arguments → Argument definitions
  - Enum values → Enum value definitions
  - Directives → Directive definitions
- **Find References**: Find all usages of:
  - Fragment definitions
  - Type definitions
  - Fields (across schema and operations)

### Code Intelligence

- **Hover Information**: Type details and descriptions for GraphQL elements
- **Works with embedded GraphQL**: Supports GraphQL in TypeScript/JavaScript template literals

### Multi-Language Support

- Pure GraphQL files (`.graphql`, `.gql`)
- Embedded GraphQL in TypeScript/JavaScript (`gql` tagged templates)
- Automatic position adjustment for embedded queries

## Installation

### From VS Code Marketplace

Coming soon - this extension will be published to the VS Code Marketplace.

### From GitHub Release

1. Download the `.vsix` file from the [latest release](https://github.com/trevor-scheer/graphql-analyzer/releases)
2. Install in VS Code:
   - Open VS Code
   - Go to Extensions view (Ctrl/Cmd+Shift+X)
   - Click the "..." menu at the top of the Extensions view
   - Select "Install from VSIX..."
   - Choose the downloaded `.vsix` file

Or install via command line:

```bash
code --install-extension graphql-lsp-*.vsix
```

### Automatic LSP Server Download

The extension automatically downloads and installs the appropriate LSP server binary for your platform on first use. No manual installation required!

Supported platforms:

- macOS (Intel and Apple Silicon)
- Linux (x86_64 and ARM64)
- Windows (x86_64)

## Configuration

The extension supports several configuration options in VS Code settings:

### Basic Settings

```json
{
  // Logging verbosity (off, messages, verbose)
  "graphql.trace.server": "off",

  // Custom path to LSP server binary (optional)
  "graphql.server.path": "/path/to/graphql-lsp",

  // Environment variables for LSP server
  "graphql.server.env": {
    "RUST_LOG": "debug",
    "OTEL_TRACES_ENABLED": "1"
  }
}
```

### Linting Configuration

Linting is configured via `.graphqlrc.yaml` in your project root:

```yaml
# Basic configuration
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"

# Lint rules
lint:
  recommended: error
  rules:
    no_deprecated: warn
    require_id_field: error
    redundant_fields: error
    unused_fields: off

# Tool-specific overrides
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off
```

See [Configuration Documentation](../../README.md#configuration) for more details.

## Usage Examples

### Goto Definition

Place your cursor on any GraphQL element and press `F12` (or right-click → "Go to Definition"):

```graphql
# Click on "UserFields" and jump to its definition
query GetUser {
  user {
    ...UserFields # F12 → jumps to fragment definition
  }
}

fragment UserFields on User {
  id
  name
}
```

### Find References

Right-click on a fragment or type definition and select "Find All References":

```graphql
# Right-click on "UserFields" to see all places it's used
fragment UserFields on User {
  id
  name
}
```

### Hover Information

Hover over any type or field to see its documentation:

```graphql
query {
  user {
    name # Hover to see: String! - The user's full name
  }
}
```

## Development Setup

### Prerequisites

- Rust toolchain (see `rust-toolchain.toml` in repo root)
- Node.js and npm

### Steps

1. **Clone and build the LSP server:**

   ```bash
   git clone https://github.com/trevor-scheer/graphql-analyzer.git
   cd graphql-lsp
   cargo build --package graphql-lsp
   ```

2. **Install extension dependencies:**

   ```bash
   cd editors/vscode
   npm install
   npm run compile
   ```

3. **Launch extension in debug mode:**

   Open the `editors/vscode` directory in VS Code and press `F5` to launch the Extension Development Host.

4. **Set custom binary path (optional):**

   The extension automatically uses `target/debug/graphql` when running from the repository. To override:

   ```bash
   export GRAPHQL_PATH=/custom/path/to/graphql
   ```

   Or set `graphql.server.path` in VS Code settings.

### Testing

1. In the Extension Development Host window, open a folder with a `.graphqlrc.yaml` config
2. Create or open a `.graphql` file
3. Test features:
   - **Validation**: Write invalid GraphQL and see error diagnostics
   - **Goto Definition**: F12 on fragment spreads, types, fields
   - **Find References**: Right-click → Find All References
   - **Hover**: Hover over types and fields

Example test query:

```graphql
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
    email
  }
}
```

## Troubleshooting

### Extension Not Working

**Check LSP server status:**

1. Open VS Code Output panel (View → Output)
2. Select "GraphQL Language Server" from dropdown
3. Look for errors or connection issues

**Common solutions:**

- Ensure `.graphqlrc.yaml` exists in your project root
- Verify schema file path is correct in config
- Check that document patterns match your GraphQL files
- Reload VS Code window (Ctrl/Cmd+Shift+P → "Developer: Reload Window")

### LSP Server Not Starting

**Check binary exists:**

```bash
# If using auto-download
ls ~/.vscode/extensions/*/graphql-lsp-*

# If using local build
ls target/debug/graphql-lsp
```

**Enable debug logging:**

```json
{
  "graphql.trace.server": "verbose",
  "graphql.server.env": {
    "RUST_LOG": "debug"
  }
}
```

### No Diagnostics Showing

**Verify configuration:**

- Check that `.graphqlrc.yaml` is valid YAML
- Ensure schema file exists and is readable
- Verify document glob patterns match your files

**Check file associations:**

- Ensure `.graphql` files are recognized
- For TypeScript/JavaScript, GraphQL must be in `gql` tagged templates

### Performance Issues

**Disable expensive lints:**

```yaml
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off
        unused_fragments: off
```

**Profile with OpenTelemetry:**

```json
{
  "graphql.server.env": {
    "OTEL_TRACES_ENABLED": "1",
    "RUST_LOG": "debug"
  }
}
```

Then view traces at http://localhost:16686 (requires Jaeger running).

## Extension Development

### Building

```bash
npm run compile    # Compile TypeScript
npm run watch      # Watch mode for development
```

### Formatting & Linting

```bash
npm run format     # Format with Prettier
npm run format:check  # Check formatting
npm run lint       # Lint with oxlint
```

### Packaging

```bash
npm run package    # Creates .vsix file
```

### Publishing

The extension is automatically published to GitHub Releases via CI. For manual publishing:

```bash
# Increment version in package.json
npm version patch  # or minor, major

# Create and push tag
git tag -a vscode-v0.1.0 -m "Release vscode v0.1.0"
git push origin vscode-v0.1.0
```

## Support

- **Issues**: [GitHub Issues](https://github.com/trevor-scheer/graphql-analyzer/issues)
- **Discussions**: [GitHub Discussions](https://github.com/trevor-scheer/graphql-analyzer/discussions)

## License

MIT OR Apache-2.0
