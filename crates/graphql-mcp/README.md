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

### Standalone Binary

```bash
graphql-mcp --workspace /path/to/project
```

### Claude Desktop Configuration

```json
{
  "mcpServers": {
    "graphql": {
      "command": "graphql",
      "args": ["mcp", "--workspace", "/path/to/project"]
    }
  }
}
```

## Available Tools

### validate_document

Validate a GraphQL document against the loaded schema. Returns syntax errors, unknown field errors, type errors, and other validation issues.

### lint_document

Run lint rules on a GraphQL document to check for best practices and code quality issues. Returns warnings about naming conventions, deprecated fields, unused variables, and other potential problems.
