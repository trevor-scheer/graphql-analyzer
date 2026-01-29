# graphql-mcp

MCP (Model Context Protocol) server for GraphQL tooling.

This crate provides AI agents with GraphQL analysis capabilities including:

- Schema-aware validation
- Linting with auto-fix suggestions
- Type information and completions
- Schema introspection from remote endpoints

## Usage

### CLI Subcommand

```bash
# Load all projects from the workspace (default)
graphql mcp --workspace /path/to/project

# Load only specific projects
graphql mcp --workspace /path/to/project --preload frontend,backend

# Don't preload any projects (load on demand via load_project tool)
graphql mcp --workspace /path/to/project --no-preload
```

### CLI Options

| Option         | Description                                                                    |
| -------------- | ------------------------------------------------------------------------------ |
| `--workspace`  | Path to the workspace directory (defaults to current directory)                |
| `--preload`    | Comma-separated list of specific projects to preload                           |
| `--no-preload` | Don't load any projects at startup. Use `load_project` tool to load on demand. |

### Claude Desktop Configuration

Add to your Claude Desktop config file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "graphql": {
      "command": "/path/to/graphql",
      "args": ["mcp", "--workspace", "/path/to/project"]
    }
  }
}
```

## Available Tools

### validate_document

Validate a GraphQL document against the loaded schema. Returns JSON with `{valid, error_count, warning_count, diagnostics[]}`.

**Parameters:**

- `document` (required): The GraphQL document source to validate
- `file_path` (optional): Virtual file path for error reporting
- `project` (optional): Project name to validate against. If not specified, uses the first loaded project.

### lint_document

Run lint rules on a GraphQL document to check for best practices and code quality issues. Returns JSON with `{issue_count, fixable_count, diagnostics[]}`.

**Parameters:**

- `document` (required): The GraphQL document source to lint
- `file_path` (optional): Virtual file path for error reporting
- `project` (optional): Project name to lint against. If not specified, uses the first loaded project.

### list_projects

List all GraphQL projects in the workspace configuration. Returns JSON array of `{name, is_loaded}`.

**Parameters:** None

### load_project

Load a specific GraphQL project by name. Use this when `--no-preload` was specified or to load a project that wasn't preloaded.

**Parameters:**

- `project` (required): The project name to load

**Returns:** JSON with `{success, project, message}`

### get_project_diagnostics

Get all diagnostics for all loaded projects. Returns JSON with `{project, total_count, file_count, files[{file, diagnostics[]}]}`.

**Parameters:** None

## Development

### Dogfooding with Test Workspaces

This repo includes test workspaces for development and testing:

```bash
# Build the CLI
cargo build

# Run MCP server against test workspace
./scripts/mcp-dev.sh

# Or use a specific project
./scripts/mcp-dev.sh test-workspace/pokemon
```

### Generate Claude Desktop Config

```bash
# Output config with resolved paths
./scripts/setup-mcp-config.sh

# For a specific workspace
./scripts/setup-mcp-config.sh test-workspace/pokemon
```

### Test Workspaces

| Workspace                  | Description                                         |
| -------------------------- | --------------------------------------------------- |
| `test-workspace`           | Multi-project config (pokemon, starwars, countries) |
| `test-workspace/pokemon`   | Comprehensive schema with types, interfaces, unions |
| `test-workspace/starwars`  | Simple Star Wars API schema                         |
| `test-workspace/countries` | Remote schema via introspection                     |

### Debugging

Enable debug logging:

```bash
RUST_LOG=graphql_mcp=debug ./scripts/mcp-dev.sh
```

### Testing MCP Tools Manually

You can test the MCP server manually using JSON-RPC:

```bash
# Start the server
./scripts/mcp-dev.sh &

# Send a tool call (in another terminal)
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"validate_document","arguments":{"document":"{ pokemon { name } }"}}}' | target/debug/graphql mcp --workspace test-workspace
```

## Architecture

The MCP server wraps `graphql-ide`'s Analysis API with multi-project support:

```
┌─────────────────────────────────────┐
│  MCP Client (Claude Desktop, etc)  │
└─────────────────┬───────────────────┘
                  │ JSON-RPC over stdio
┌─────────────────▼───────────────────┐
│  GraphQLToolRouter (MCP Handler)    │
│  - validate_document                │
│  - lint_document                    │
│  - list_projects                    │
│  - load_project                     │
│  - get_project_diagnostics          │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│  McpService                         │
│  - One AnalysisHost per project     │
│  - Loads workspace config           │
│  - Supports selective preloading    │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│  graphql-ide Analysis               │
│  - Salsa-based incremental queries  │
│  - Schema + document validation     │
└─────────────────────────────────────┘
```

### Multi-Project Support

The MCP server supports GraphQL workspaces with multiple projects (as defined in `.graphqlrc.yaml`):

- By default, all projects are loaded at startup
- Use `--no-preload` for large workspaces to defer loading
- Use `--preload` to selectively preload specific projects
- The `load_project` tool allows loading additional projects on demand
- Each project has its own `AnalysisHost` with independent schema and documents

## Future Enhancements

- [ ] `get_hover_info` - Type information at a position
- [ ] `get_completions` - Code completions at a position
- [ ] `get_schema_types` - List all types in the schema
- [ ] `introspect_endpoint` - Fetch schema from a GraphQL endpoint
- [ ] Resource exposure for schema files
- [ ] Embedded mode in LSP server (shared Analysis cache)
