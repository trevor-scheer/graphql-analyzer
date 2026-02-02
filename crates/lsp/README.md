# graphql-lsp

Language Server Protocol (LSP) implementation providing IDE features for GraphQL.

## Features

- **Real-Time Validation**: Instant feedback with project-wide diagnostics
- **Goto Definition**: Navigate to fragments, types, fields, variables, directives, enum values, and arguments
- **Find References**: Find all usages of fragments and type definitions across the project
- **Hover Information**: Display type information and descriptions
- **Embedded GraphQL**: Full support for TypeScript/JavaScript with position mapping
- **Multi-Project**: Works with single and multi-project configurations
- **Configurable Linting**: Custom lint rules with tool-specific configuration

## Installation

The LSP server is typically installed via editor extensions. For VSCode, see the [VSCode extension](../../editors/vscode/).

### Via Cargo

```bash
cargo install graphql-lsp
```

### From Binary Release

Download the appropriate binary from the [releases page](https://github.com/trevor-scheer/graphql-analyzer/releases):

- macOS (Intel): `graphql-lsp-x86_64-apple-darwin.tar.xz`
- macOS (Apple Silicon): `graphql-lsp-aarch64-apple-darwin.tar.xz`
- Linux (x86_64): `graphql-lsp-x86_64-unknown-linux-gnu.tar.xz`
- Linux (ARM64): `graphql-lsp-aarch64-unknown-linux-gnu.tar.xz`
- Windows: `graphql-lsp-x86_64-pc-windows-msvc.zip`

### Custom Binary Path

Set in your editor configuration:

**VSCode** (`settings.json`):

```json
{
  "graphql.server.path": "/path/to/graphql-lsp"
}
```

**Neovim** (`init.lua`):

```lua
require('lspconfig').graphql.setup {
  cmd = { '/path/to/graphql-lsp' }
}
```

## Getting Started

### VSCode

1. Install the [GraphQL LSP extension](../../editors/vscode/)
2. Create a `.graphqlrc.yml` in your project root:

```yaml
schema: schema.graphql
documents: src/**/*.{graphql,ts,tsx}
```

3. Open a GraphQL file and start editing

### Neovim

Using [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig):

```lua
require('lspconfig').graphql.setup {
  cmd = { 'graphql-lsp' },
  filetypes = { 'graphql', 'typescriptreact', 'javascriptreact' },
  root_dir = require('lspconfig.util').root_pattern('.graphqlrc*', 'graphql.config.*'),
}
```

### Other Editors

The LSP server communicates via stdin/stdout using the standard LSP protocol. Configure your editor's LSP client to:

1. Launch `graphql-lsp` as the server command
2. Associate it with GraphQL file types
3. Set the root directory to where `.graphqlrc` is located

## Configuration

The LSP uses `.graphqlrc` configuration files. It searches for:

- `.graphqlrc` (YAML or JSON)
- `.graphqlrc.yml` / `.graphqlrc.yaml`
- `.graphqlrc.json`

### Basic Configuration

```yaml
schema: schema.graphql
documents: src/**/*.graphql
```

### Multi-Project Configuration

```yaml
projects:
  api:
    schema: api/schema.graphql
    documents: api/**/*.graphql
  client:
    schema: client/schema.graphql
    documents: client/**/*.{graphql,tsx}
```

### Lint Configuration

Lint configuration lives under `extensions.lint`. Rule names use camelCase:

```yaml
extensions:
  # Base lint config
  lint:
    extends: recommended
    rules:
      noDeprecated: warn
      uniqueNames: error

  # LSP-specific overrides
  lsp:
    lint:
      rules:
        unusedFields: off
```

See [graphql-linter](../graphql-linter/README.md) for available rules.

### Remote Schema Support

Load schemas from GraphQL endpoints via introspection:

```yaml
schema: https://api.example.com/graphql
documents: src/**/*.graphql
```

The LSP automatically fetches and caches the schema.

## Implemented Features

### Diagnostics

Real-time validation with accurate error reporting:

- **Apollo Compiler Validation**: Full GraphQL spec compliance
- **Custom Linting**: Configurable rules with severity levels
- **Project-Wide**: Errors shown across all open documents
- **Position Accurate**: Correct line/column for embedded GraphQL

**Example:**

```graphql
query GetUser {
  user {
    unknownField # Error: Cannot query field "unknownField" on type "User"
  }
}
```

### Goto Definition

Navigate to definitions by clicking or using keyboard shortcuts:

**Supported:**

- Fragment spreads → Fragment definitions
- Operation names → Operation definitions
- Type references → Type definitions (in fragments, inline fragments, implements, union members, field types, variable types)
- Field references → Schema field definitions
- Variable references → Operation variable definitions
- Argument names → Schema argument definitions
- Enum values → Enum value definitions
- Directive names → Directive definitions
- Directive arguments → Directive argument definitions

**Works in:**

- Pure GraphQL files (`.graphql`, `.gql`)
- Embedded GraphQL in TypeScript/JavaScript
- Cross-file navigation

**Example:**

```graphql
fragment UserFields on User {
  id
  name
}

query GetUser {
  user {
    ...UserFields # Ctrl+Click → jumps to UserFields definition
  }
}
```

### Find References

Find all usages of GraphQL elements:

**Supported:**

- Fragment definitions → All fragment spreads
- Type definitions → All usages in field types, union members, implements clauses, input fields, arguments

**Respects:**

- Include/exclude declaration context from client
- List and NonNull type wrappers

**Example:**

Find all places where the `User` type is used:

- Field types: `user: User`
- Union members: `SearchResult = User | Post`
- Implements clauses: `Admin implements User`

### Hover Information

Display type information and descriptions:

**Shows:**

- Type information for fields
- Schema descriptions
- Deprecation warnings
- Argument types and defaults

**Example:**

```graphql
query {
  user {
    name # Hover: String! - The user's full name
  }
}
```

### Embedded GraphQL Support

Full support for TypeScript/JavaScript with position mapping:

**Supported patterns:**

```typescript
import { gql } from "graphql-tag";

const query = gql`
  query GetUser($id: ID!) {
    user(id: $id) {
      id
      name
    }
  }
`;
```

**Features:**

- Accurate diagnostics at correct positions
- Goto definition from embedded GraphQL
- Hover information in template literals
- Find references across files

## Running the LSP Server

### Standard Mode

The LSP communicates via stdin/stdout:

```bash
graphql-lsp
```

### Debug Mode

Enable logging to stderr:

```bash
RUST_LOG=debug graphql-lsp 2> lsp.log
```

Log levels: `error`, `warn`, `info`, `debug`, `trace`

### With OpenTelemetry Tracing

Build with the `otel` feature for performance analysis:

```bash
cargo build --features otel --release
```

Run with tracing enabled:

```bash
# Start Jaeger
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest

# Run LSP with tracing
OTEL_TRACES_ENABLED=1 ./target/release/graphql-lsp
```

View traces at [http://localhost:16686](http://localhost:16686)

## LSP Protocol Implementation

### Text Document Synchronization

- `textDocument/didOpen` - Load and validate document
- `textDocument/didChange` - Incremental updates and re-validation
- `textDocument/didClose` - Clean up document state
- `textDocument/didSave` - Re-validate on save

### Language Features

- `textDocument/definition` - Goto definition
- `textDocument/references` - Find references
- `textDocument/hover` - Hover information
- `textDocument/publishDiagnostics` - Real-time validation errors

### Workspace

- `workspace/didChangeWatchedFiles` - React to file changes
- `workspace/didChangeConfiguration` - Update settings

## Performance Characteristics

### Incremental Updates

Only re-validates changed documents:

```
File opened    → Validate document
File changed   → Re-validate document only
File saved     → Re-validate document only
Schema changed → Re-validate all documents
```

### Fast Rules Only

Expensive project-wide lints disabled by default:

```yaml
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off # Too expensive for real-time
```

Enable expensive rules in the CLI for CI/CD:

```yaml
extensions:
  cli:
    lint:
      rules:
        unused_fields: error # Run in CI only
```

### Concurrent Requests

Multiple LSP requests handled in parallel using async Rust and concurrent data structures.

## Configuration Examples

### TypeScript Project with Remote Schema

```yaml
schema: https://api.example.com/graphql
documents:
  - "src/**/*.{ts,tsx}"
  - "!src/**/*.test.ts"
extensions:
  lint:
    extends: recommended
  lsp:
    lint:
      rules:
        noDeprecated: warn
```

### Monorepo with Multiple Projects

```yaml
projects:
  web:
    schema: packages/web/schema.graphql
    documents: packages/web/src/**/*.{graphql,tsx}
    extensions:
      lint:
        extends: recommended
  mobile:
    schema: packages/mobile/schema.graphql
    documents: packages/mobile/src/**/*.{graphql,ts}
    extensions:
      lint:
        extends: recommended
  api:
    schema: packages/api/schema/**/*.graphql
    documents: packages/api/src/**/*.graphql
    extensions:
      lint:
        extends: recommended
```

### Custom Extract Configuration

```yaml
schema: schema.graphql
documents: "src/**/*.tsx"
extensions:
  extractConfig:
    tagIdentifiers: ["gql", "graphql", "query"]
    modules: ["graphql-tag", "@apollo/client", "custom-gql"]
    allowGlobalIdentifiers: true
```

## Environment Variables

- `RUST_LOG` - Log level (`error`, `warn`, `info`, `debug`, `trace`)
- `OTEL_TRACES_ENABLED` - Enable OpenTelemetry tracing (`1` or `true`)
- `OTEL_EXPORTER_OTLP_ENDPOINT` - OTLP endpoint (default: `http://localhost:4317`)

## Differences from CLI

The LSP and CLI share core validation logic but are optimized differently:

### LSP (This Server)

- **Real-time feedback**: Validates as you type
- **Incremental updates**: Only re-validates changed files
- **Editor integration**: VSCode, Neovim, etc.

### CLI

- **Batch processing**: Validates all files at once
- **CI/CD optimized**: Exit codes, JSON output
- **No incremental updates**: Full project validation each run

Configure independently:

```yaml
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off # Disable in LSP
  cli:
    lint:
      rules:
        unused_fields: error # Enable in CLI
```

## Troubleshooting

### LSP Not Starting

Check logs in your editor:

**VSCode**: View → Output → GraphQL Language Server

**Neovim**: `:LspLog`

### Configuration Not Found

Ensure `.graphqlrc.yml` is in your workspace root:

```bash
ls -la .graphqlrc.yml
```

### Schema Loading Errors

Test schema loading manually:

```bash
RUST_LOG=debug graphql-lsp 2> lsp.log
# Check lsp.log for schema loading errors
```

### No Diagnostics

Verify documents match the pattern in config:

```yaml
documents: "src/**/*.graphql" # Must match your file locations
```

### Embedded GraphQL Not Working

Check import tracking:

```typescript
// ✓ Will work (gql from recognized module)
import { gql } from "graphql-tag";

// ✗ Won't work (unknown module)
import { gql } from "custom-unknown-module";
```

Add custom modules:

```yaml
extensions:
  extractConfig:
    modules: ["graphql-tag", "custom-module"]
```

## Building from Source

```bash
# Clone repository
git clone https://github.com/trevor-scheer/graphql-analyzer
cd graphql-lsp

# Build LSP server
cargo build --package graphql-lsp --release

# Binary at target/release/graphql-lsp
./target/release/graphql-lsp
```

### Development Mode

```bash
# Run with logging
RUST_LOG=debug cargo run --package graphql-lsp 2> lsp.log

# Build with OpenTelemetry
cargo build --package graphql-lsp --features otel
```

## License

MIT OR Apache-2.0
