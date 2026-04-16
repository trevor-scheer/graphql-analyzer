# MCP Tools: Schema Exploration, Document Analysis, and Utilities

**Issue**: [#444](https://github.com/trevor-scheer/graphql-analyzer/pull/444)
**Date**: 2026-03-29

## Overview

Add 6 new MCP tools to make the server more useful for AI agents working with GraphQL projects. These tools fall into three categories: schema exploration (understanding what's in the schema), document analysis (understanding operations), and utilities (introspecting remote endpoints).

Two tools from the original issue (`get_completions`, `goto_definition`) are already implemented. One (`format_document`) is deferred to a separate issue as it requires building a full formatter.

## Tools

### 1. `get_schema_types`

List all types in the loaded schema with their kind and basic metadata.

**Parameters:**

- `project` (optional string): Project name. Defaults to first/only loaded project.
- `kind` (optional string): Filter by type kind — `"object"`, `"interface"`, `"union"`, `"enum"`, `"scalar"`, `"input_object"`. If omitted, returns all types.

**Returns:**

```json
{
  "types": [
    {
      "name": "User",
      "kind": "object",
      "description": "A user account",
      "field_count": 5,
      "implements": ["Node"],
      "is_extension": false
    }
  ],
  "count": 42,
  "stats": {
    "objects": 20,
    "interfaces": 3,
    "unions": 2,
    "enums": 8,
    "scalars": 5,
    "input_objects": 4,
    "total_fields": 150,
    "directives": 3
  }
}
```

**Implementation:** New `Analysis::schema_type_list()` method that iterates `schema_types()` from HIR and returns a lightweight summary. The `stats` field reuses the existing `schema_stats()` method.

### 2. `get_type_info`

Get full details about a specific named type including fields, arguments, interfaces, directives, enum values, and union members.

**Parameters:**

- `type_name` (string, required): The name of the type to look up.
- `project` (optional string): Project name.

**Returns (for an object type):**

```json
{
  "name": "User",
  "kind": "object",
  "description": "A user account",
  "implements": ["Node"],
  "fields": [
    {
      "name": "id",
      "type": "ID!",
      "description": "Unique identifier",
      "arguments": [],
      "is_deprecated": false,
      "deprecation_reason": null,
      "directives": []
    },
    {
      "name": "posts",
      "type": "[Post!]!",
      "description": null,
      "arguments": [{ "name": "first", "type": "Int", "description": null, "default_value": null }],
      "is_deprecated": false,
      "deprecation_reason": null,
      "directives": []
    }
  ],
  "directives": [{ "name": "key", "arguments": [{ "name": "fields", "value": "\"id\"" }] }],
  "enum_values": [],
  "union_members": []
}
```

For enums, `enum_values` is populated. For unions, `union_members` is populated. For interfaces, `fields` and `implements` (extended by) could both be present.

**Implementation:** New `Analysis::type_info()` method that looks up a single type in `schema_types()` and converts the HIR `TypeDef` to an MCP-friendly representation. Uses `format_type_ref()` from `ide/src/helpers.rs` to render `TypeRef` as strings like `"[Post!]!"`.

### 3. `get_schema_sdl`

Return the full merged schema as SDL text. This reconstructs SDL from the resolved HIR types (with extensions merged), giving agents a single canonical view of the schema.

**Parameters:**

- `project` (optional string): Project name.

**Returns:**

```json
{
  "sdl": "type Query {\n  user(id: ID!): User\n  ...\n}\n\ntype User implements Node {\n  id: ID!\n  ...\n}\n...",
  "type_count": 42
}
```

**Implementation:** New `sdl_printer` module in the `mcp` crate (or `ide` crate if reuse is likely). Walks the `TypeDefMap` from `schema_types()` and emits each type definition as valid SDL:

- Scalars: `scalar DateTime`
- Enums: `enum Status { ACTIVE INACTIVE }` (with descriptions and deprecation)
- Input objects: `input CreateUserInput { name: String! }`
- Objects/Interfaces: type with fields, arguments, implements clauses
- Unions: `union SearchResult = User | Post`
- Includes descriptions as `"""..."""` block strings
- Includes directives on types and fields
- Sorted alphabetically by type name, with `Query`/`Mutation`/`Subscription` first

The SDL printer lives in `crates/mcp/src/sdl_printer.rs` since it's specific to the MCP use case (providing context to agents). If the LSP or CLI later needs it, it can be moved to `ide`.

### 4. `get_operations`

Extract all operations from the loaded project with their names, types, variables, and fragment dependencies.

**Parameters:**

- `project` (optional string): Project name.
- `file_path` (optional string): If provided, only return operations from this file.

**Returns:**

```json
{
  "operations": [
    {
      "name": "GetUser",
      "operation_type": "query",
      "file": "/path/to/queries.graphql",
      "variables": [{ "name": "id", "type": "ID!", "default_value": null }],
      "fragment_dependencies": ["UserFields", "AddressFields"]
    }
  ],
  "count": 15
}
```

**Implementation:** New `Analysis::operations_summary()` method. Uses `all_operations()` for the operation list and `operation_body()` + fragment spread walking for dependency extraction. The fragment dependency extraction walks the operation body's selection sets to find `FragmentSpread` nodes, then transitively resolves their dependencies via `all_fragments()`.

### 5. `get_query_complexity`

Calculate complexity scores for operations in the project.

**Parameters:**

- `project` (optional string): Project name.
- `operation_name` (optional string): If provided, only return complexity for this operation. Otherwise returns all.

**Returns:**

```json
{
  "operations": [
    {
      "operation_name": "GetUser",
      "operation_type": "query",
      "total_complexity": 15,
      "depth": 4,
      "breakdown": [
        { "path": "user", "complexity": 1, "multiplier": null },
        { "path": "user.posts", "complexity": 10, "multiplier": 10 }
      ],
      "warnings": ["Nested pagination detected: user.posts.comments"],
      "file": "/path/to/queries.graphql"
    }
  ],
  "count": 1
}
```

**Implementation:** Wraps the existing `Analysis::complexity_analysis()` method. The IDE type `ComplexityAnalysis` already has all the fields needed. Add MCP wrapper types and convert.

### 6. `introspect_endpoint`

Fetch a schema from a remote GraphQL endpoint via introspection and return the SDL.

**Parameters:**

- `url` (string, required): The GraphQL endpoint URL.
- `headers` (optional object): Additional HTTP headers (e.g., `{"Authorization": "Bearer token"}`).

**Returns:**

```json
{
  "sdl": "type Query { ... }",
  "url": "https://api.example.com/graphql"
}
```

**Implementation:** Wraps `graphql_introspect::introspect_url_to_sdl()`. This is the only async tool in this batch. The existing introspect crate handles the HTTP request and SDL conversion. We need to add header support — the current `execute_introspection()` function doesn't accept custom headers. Add an optional `headers` parameter to the introspection function.

## Architecture

### New Files

- `crates/mcp/src/sdl_printer.rs` — SDL reconstruction from HIR TypeDefMap

### Modified Files

- `crates/mcp/src/types.rs` — New param/result types for all 6 tools
- `crates/mcp/src/tools.rs` — 6 new `#[tool]` methods on `GraphQLToolRouter`
- `crates/mcp/src/service.rs` — 6 new service methods
- `crates/ide/src/analysis.rs` — New methods: `schema_type_list()`, `type_info()`, `operations_summary()`
- `crates/introspect/src/lib.rs` — Add header support to `execute_introspection()` / `introspect_url_to_sdl()`

### New IDE Analysis Methods

Three new methods on `Analysis`:

1. **`schema_type_list(kind_filter: Option<&str>) -> Vec<SchemaTypeEntry>`** — Lightweight type listing from `schema_types()`. Returns name, kind, description, field count, implements list.

2. **`type_info(type_name: &str) -> Option<TypeInfo>`** — Full type details from `schema_types()` lookup. Converts HIR `TypeDef` fields/args/directives to IDE POD types.

3. **`operations_summary(file_filter: Option<&FilePath>) -> Vec<OperationSummary>`** — Operation extraction with fragment dependency resolution. Uses `all_operations()` for the list and `operation_body()` selection set walking for fragment deps.

### Data Flow

```
MCP Tool handler (#[tool] method)
  → McpService method (project resolution, file path normalization)
    → Analysis method (query over Salsa DB)
      → HIR queries (schema_types, all_operations, operation_body)
        → Salsa cache (memoized, incremental)
```

### SDL Printer Design

The SDL printer is a standalone function: `fn print_schema_sdl(types: &TypeDefMap) -> String`

It iterates the TypeDefMap (sorted), and for each type emits:

- Description (triple-quoted block string if multiline, inline `"..."` if single line)
- Type keyword + name + implements clause (if any)
- Fields with arguments, types, descriptions, directives, deprecation
- Enum values with descriptions and deprecation
- Union members
- Directives on types

It does NOT print:

- Directive definitions (could be added later)
- Schema definition block (the root types are implicit from `Query`/`Mutation`/`Subscription` type names)
- Built-in scalars (`String`, `Int`, `Float`, `Boolean`, `ID`)

## Testing

Each tool gets integration tests following the existing pattern in `crates/mcp/`:

1. **`get_schema_types`** — Load a test schema, verify type listing with kind filter
2. **`get_type_info`** — Verify field details, arguments, enum values, union members, directives for different type kinds
3. **`get_schema_sdl`** — Verify output is valid SDL that round-trips (parse the output and compare type counts)
4. **`get_operations`** — Load documents with operations and fragments, verify extraction and dependency resolution
5. **`get_query_complexity`** — Verify complexity scores match expected values for known queries
6. **`introspect_endpoint`** — Unit test the SDL conversion; skip live HTTP in CI (or mock with a test server)

The SDL printer gets its own unit tests with snapshot-style assertions for various type kinds.

## Out of Scope

- `format_document` — Requires building a full GraphQL formatter (separate issue)
- `get_completions` — Already implemented
- `goto_definition` — Already implemented
- Directive definition listing in `get_schema_types` — Could be added later
- Schema definition block in SDL output — Root types are implicit
