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
graphql mcp --workspace /path/to/project
```

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

Validate a GraphQL document against the loaded schema. Returns syntax errors, unknown field errors, type errors, and other validation issues.

**Parameters:**
- `document` (required): The GraphQL document source to validate
- `file_path` (optional): Virtual file path for error reporting

### lint_document

Run lint rules on a GraphQL document to check for best practices and code quality issues. Returns warnings about naming conventions, deprecated fields, unused variables, and other potential problems.

**Parameters:**
- `document` (required): The GraphQL document source to lint
- `file_path` (optional): Virtual file path for error reporting

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

| Workspace | Description |
|-----------|-------------|
| `test-workspace` | Multi-project config (pokemon, starwars, countries) |
| `test-workspace/pokemon` | Comprehensive schema with types, interfaces, unions |
| `test-workspace/starwars` | Simple Star Wars API schema |
| `test-workspace/countries` | Remote schema via introspection |

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

The MCP server wraps `graphql-ide`'s Analysis API:

```
┌─────────────────────────────────────┐
│  MCP Client (Claude Desktop, etc)  │
└─────────────────┬───────────────────┘
                  │ JSON-RPC over stdio
┌─────────────────▼───────────────────┐
│  GraphQLToolRouter (MCP Handler)    │
│  - validate_document                │
│  - lint_document                    │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│  McpService                         │
│  - Wraps AnalysisHost               │
│  - Loads workspace config           │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│  graphql-ide Analysis               │
│  - Salsa-based incremental queries  │
│  - Schema + document validation     │
└─────────────────────────────────────┘
```

## Future Enhancements

- [ ] `get_hover_info` - Type information at a position
- [ ] `get_completions` - Code completions at a position
- [ ] `get_schema_types` - List all types in the schema
- [ ] `introspect_endpoint` - Fetch schema from a GraphQL endpoint
- [ ] Resource exposure for schema files
- [ ] Embedded mode in LSP server (shared Analysis cache)
