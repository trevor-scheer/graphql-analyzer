# graphql-cli

Command-line tool for validating and linting GraphQL projects.

## Features

- **Validate Command**: Run Apollo compiler validation on schemas and documents
- **Lint Command**: Execute custom lint rules with configurable severity
- **Watch Mode**: Continuous validation during development
- **Multiple Output Formats**: Human-readable, JSON, and GitHub Actions
- **CI/CD Integration**: Exit codes and machine-readable output for automation
- **Multi-Project Support**: Works with single and multi-project configurations

## Installation

### Via Installation Script (Recommended)

**macOS and Linux:**

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/trevor-scheer/graphql-analyzer/releases/latest/download/graphql-cli-installer.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://github.com/trevor-scheer/graphql-analyzer/releases/latest/download/graphql-cli-installer.ps1 | iex
```

### Via Cargo

```bash
cargo install --git https://github.com/trevor-scheer/graphql-analyzer graphql-cli
```

### From Binary Release

Download the appropriate binary for your platform from the [releases page](https://github.com/trevor-scheer/graphql-analyzer/releases):

- macOS (Intel): `graphql-cli-x86_64-apple-darwin.tar.xz`
- macOS (Apple Silicon): `graphql-cli-aarch64-apple-darwin.tar.xz`
- Linux (x86_64): `graphql-cli-x86_64-unknown-linux-gnu.tar.xz`
- Linux (ARM64): `graphql-cli-aarch64-unknown-linux-gnu.tar.xz`
- Windows: `graphql-cli-x86_64-pc-windows-msvc.zip`

## Getting Started

### Validate GraphQL

Validate documents against your schema using Apollo compiler:

```bash
# Auto-discover and use .graphqlrc configuration
graphql validate

# Specify config file
graphql --config .graphqlrc.yml validate

# Specify project in multi-project config
graphql --project my-api validate
```

### Lint GraphQL

Run custom lint rules with configurable severity:

```bash
# Run lints with configured rules
graphql lint

# Watch mode - re-lint on file changes
graphql lint --watch
```

### Output Formats

```bash
# Human-readable (default, colorized)
graphql validate

# JSON output for CI/CD
graphql validate --format json

# GitHub Actions annotations
graphql validate --format github
```

## Commands

### validate

Validates GraphQL documents against a schema using Apollo compiler validation.

```bash
graphql validate [OPTIONS]
```

**Options:**

- `--format <FORMAT>` - Output format: `human` (default), `json`, `github`
- `--watch` - Watch for file changes and re-validate
- `--config <PATH>` - Path to config file (auto-discovered by default)
- `--project <NAME>` - Project name for multi-project configs

**Exit codes:**

- `0` - No validation errors
- `1` - Validation errors found

**Examples:**

```bash
# Basic validation
graphql validate

# JSON output for CI
graphql validate --format json

# Watch mode for development
graphql validate --watch

# Specific project
graphql --project backend validate
```

### lint

Runs custom lint rules with configurable severity levels.

```bash
graphql lint [OPTIONS]
```

**Options:**

- `--format <FORMAT>` - Output format: `human` (default), `json`, `github`
- `--watch` - Watch for file changes and re-lint
- `--config <PATH>` - Path to config file (auto-discovered by default)
- `--project <NAME>` - Project name for multi-project configs

**Exit codes:**

- `0` - No lint errors
- `1` - Lint errors found (warnings don't cause non-zero exit)

**Examples:**

```bash
# Basic linting
graphql lint

# JSON output for CI
graphql lint --format json

# Watch mode for development
graphql lint --watch

# Specific project
graphql --project frontend lint
```

## Output Formats

### Human (Default)

Colorized, human-readable output with context:

```
✗ Validation error in src/queries.graphql:5:3
  Cannot query field "invalidField" on type "User"

  3 |   user(id: $id) {
  4 |     id
  5 |     invalidField
    |     ^^^^^^^^^^^^
  6 |   }

✓ 12 files validated, 1 error found
```

### JSON

Machine-readable JSON output:

```json
{
  "success": false,
  "errors": [
    {
      "file": "src/queries.graphql",
      "message": "Cannot query field \"invalidField\" on type \"User\"",
      "severity": "error",
      "line": 5,
      "column": 3
    }
  ],
  "warnings": []
}
```

### GitHub

GitHub Actions annotation format:

```
::error file=src/queries.graphql,line=5,col=3::Cannot query field "invalidField" on type "User"
```

Errors and warnings appear as annotations in GitHub pull requests.

## Configuration

The CLI uses standard GraphQL configuration files (YAML or JSON only). It searches for these files in order:

- `.graphqlrc.yml` / `.graphqlrc.yaml`
- `.graphqlrc.json`
- `.graphqlrc` (YAML or JSON, auto-detected)
- `graphql.config.yml` / `graphql.config.yaml`
- `graphql.config.json`

**Note:** JavaScript/TypeScript configs (`graphql.config.js`, `graphql.config.ts`) are not supported. See [config README](../config/README.md#note-on-javascripttypescript-configs) for migration guidance.

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
    documents: client/**/*.graphql
```

### Lint Configuration

```yaml
# Top-level lint config
lint:
  recommended: error
  rules:
    no_deprecated: warn
    unique_names: error

# CLI-specific overrides
extensions:
  cli:
    lint:
      rules:
        unused_fields: error
```

See [graphql-linter](../graphql-linter/README.md) for available rules and configuration options.

## Use Cases

### CI/CD Integration

#### GitHub Actions

```yaml
name: GraphQL Validation
on: [pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install CLI
        run: |
          curl --proto '=https' --tlsv1.2 -LsSf \
            https://github.com/trevor-scheer/graphql-analyzer/releases/latest/download/graphql-cli-installer.sh | sh
      - name: Validate GraphQL
        run: graphql validate --format github
      - name: Lint GraphQL
        run: graphql lint --format github
```

#### GitLab CI

```yaml
graphql-validate:
  script:
    - graphql validate --format json
  artifacts:
    reports:
      junit: graphql-validation-report.json
```

#### Generic CI

```bash
# Install
curl -LsSf https://github.com/trevor-scheer/graphql-analyzer/releases/latest/download/graphql-cli-installer.sh | sh

# Validate
graphql validate --format json > results.json

# Check exit code
if [ $? -ne 0 ]; then
  echo "GraphQL validation failed"
  exit 1
fi
```

### Pre-commit Hook

Using [husky](https://github.com/typicode/husky):

```json
{
  "husky": {
    "hooks": {
      "pre-commit": "graphql validate && graphql lint"
    }
  }
}
```

Using [cargo-husky](https://github.com/rhysd/cargo-husky):

```toml
[dev-dependencies]
cargo-husky = { version = "1", features = ["user-hooks"] }
```

Create `.cargo-husky/hooks/pre-commit`:

```bash
#!/bin/sh
graphql validate && graphql lint
```

### Development Workflow

Watch mode for continuous validation:

```bash
# Terminal 1: Run dev server
npm run dev

# Terminal 2: Watch GraphQL
graphql validate --watch

# Terminal 3: Watch lints
graphql lint --watch
```

## Examples

### Basic Validation

```bash
$ graphql validate

✓ Validating GraphQL project...
✓ Schema loaded: schema.graphql
✓ Found 15 documents
✓ All files validated successfully
```

### Validation with Errors

```bash
$ graphql validate

✗ Validation error in src/queries.graphql:5:3
  Cannot query field "invalidField" on type "User"

✓ 15 files validated, 1 error found
```

### JSON Output

```bash
$ graphql validate --format json

{
  "success": false,
  "files_validated": 15,
  "errors": [
    {
      "file": "src/queries.graphql",
      "message": "Cannot query field \"invalidField\" on type \"User\"",
      "severity": "error",
      "line": 5,
      "column": 3
    }
  ]
}
```

### Watch Mode

```bash
$ graphql validate --watch

✓ Watching for changes...
✓ Initial validation: 15 files, no errors

[12:34:56] File changed: src/queries.graphql
✓ Revalidating...
✗ Validation error in src/queries.graphql:8:5
  Unknown argument "invalidArg" on field "user"

[12:35:30] File changed: src/queries.graphql
✓ Revalidating...
✓ All files validated successfully
```

### Multi-Project

```bash
$ graphql --project api validate

✓ Validating project: api
✓ Schema loaded: api/schema.graphql
✓ Found 8 documents
✓ All files validated successfully

$ graphql --project client validate

✓ Validating project: client
✓ Schema loaded: client/schema.graphql
✓ Found 12 documents
✓ All files validated successfully
```

## Environment Variables

- `GRAPHQL_CONFIG` - Override config file path
- `RUST_LOG` - Set log level (`error`, `warn`, `info`, `debug`, `trace`)
- `NO_COLOR` - Disable colored output (any value)
- `CLICOLOR` - Set to `0` to disable colors, `1` to enable
- `CLICOLOR_FORCE` - Set to `1` to force colors even when not a TTY

```bash
RUST_LOG=debug graphql validate

# Disable colors via environment
NO_COLOR=1 graphql validate

# Or use flags
graphql validate --no-color
graphql validate --color
```

Color priority (highest to lowest):

1. `--color` / `--no-color` flags
2. `NO_COLOR` environment variable
3. `CLICOLOR_FORCE` environment variable
4. `CLICOLOR` environment variable
5. Auto-detect (colors enabled if stdout is a TTY)

See [NO_COLOR standard](https://no-color.org/) and [CLICOLOR spec](https://bixense.com/clicolors/) for details

## Architecture

The CLI uses a modern Salsa-based incremental computation architecture:

- **Shared Logic**: Uses the same validation and linting engine as the LSP
- **Single Source of Truth**: No code duplication between CLI and LSP
- **Incremental Computation**: Salsa automatically caches and reuses computations
- **Type-Safe**: Rust's type system ensures correctness

### Architecture Layers

```
graphql-cli (this crate)
    ↓ uses
graphql-ide (editor API)
    ↓ uses
graphql-analysis (validation + linting queries)
    ↓ uses
graphql-hir (semantic queries)
    ↓ uses
graphql-syntax (parsing)
    ↓ uses
graphql-db (Salsa database)
```

### Benefits

- ✅ **Consistent**: CLI and LSP produce identical results
- ✅ **Efficient**: Salsa caching minimizes redundant work
- ✅ **Maintainable**: Single codebase for all GraphQL analysis
- ✅ **Extensible**: Easy to add new validation rules and features

## Differences from LSP

The CLI and LSP share the same core validation and linting logic but are optimized for different use cases:

### CLI (This Tool)

- **Batch processing**: Validates all files at once
- **CI/CD optimized**: Exit codes, JSON output, GitHub annotations
- **Expensive rules enabled**: Project-wide lints like `unused_fields`
- **No incremental updates**: Full project validation each run

### LSP (Language Server)

- **Real-time feedback**: Validates as you type
- **Editor integration**: VSCode, Neovim, etc.
- **Fast rules only**: Expensive project-wide lints disabled by default
- **Incremental updates**: Only re-validates changed files

Both can be configured independently via tool-specific config:

```yaml
extensions:
  cli:
    lint:
      rules:
        unused_fields: error # Enable in CLI
  lsp:
    lint:
      rules:
        unused_fields: off # Disable in LSP
```

## Building from Source

```bash
# Clone repository
git clone https://github.com/trevor-scheer/graphql-analyzer
cd graphql-lsp

# Build CLI
cargo build --package graphql-cli --release

# Binary at target/release/graphql
./target/release/graphql validate
```

## License

MIT OR Apache-2.0
