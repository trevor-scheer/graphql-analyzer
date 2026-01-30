# GraphQL Configuration Schema

This directory contains a JSON Schema for `.graphqlrc` configuration files used by GraphQL LSP and related tools.

## What is it?

The [graphqlrc.schema.json](./crates/graphql-config/schema/graphqlrc.schema.json) file provides IDE validation, autocompletion, and documentation for GraphQL configuration files.

## How to Use

### VSCode

Add a comment at the top of your `.graphqlrc.yaml` or `.graphqlrc.yml` file:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/crates/graphql-config/schem./crates/graphql-config/schema/graphqlrc.schema.json
schema: schema.graphql
documents: "**/*.{graphql,gql,ts,tsx}"
```

Or configure it globally in your VSCode settings (`.vscode/settings.json`):

```json
{
  "yaml.schemas": {
    "./crates/graphql-config/schem./crates/graphql-config/schema/graphqlrc.schema.json": [
      ".graphqlrc",
      ".graphqlrc.yaml",
      ".graphqlrc.yml",
      "graphql.config.yaml",
      "graphql.config.yml"
    ]
  },
  "json.schemas": [
    {
      "fileMatch": [".graphqlrc.json", ".graphqlrc"],
      "url": "./crates/graphql-config/schem./crates/graphql-config/schema/graphqlrc.schema.json"
    }
  ]
}
```

### JetBrains IDEs (IntelliJ, WebStorm, etc.)

1. Open Settings → Languages & Frameworks → Schemas and DTDs → JSON Schema Mappings
2. Add a new mapping:
   - Name: GraphQL Configuration
   - Schema file or URL: Point to `graphqlrc.schema.json`
   - Schema version: JSON Schema version 7
   - File path pattern: `.graphqlrc*`, `graphql.config.*`

### Other Editors

Most editors with YAML/JSON support can use the modeline comment:

```yaml
# yaml-language-server: $schema=./crates/graphql-config/schema/graphqlrc.schema.json
```

## Configuration Examples

### Single Project

```yaml
schema: schema.graphql
documents: "**/*.{graphql,gql,ts,tsx,js,jsx}"
extensions:
  project:
    lint: "recommended"
```

### Multi-Project

```yaml
projects:
  api:
    schema: api/schema.graphql
    documents: "api/**/*.graphql"
    extensions:
      project:
        lint:
          recommended: error
          no_deprecated: off

  client:
    schema: client/schema.graphql
    documents: "client/**/*.{ts,tsx}"
    extensions:
      extractConfig:
        tagIdentifiers: ["gql"]
        modules: ["@apollo/client"]
      project:
        lint: "recommended"
```

### Custom Extract Configuration

```yaml
schema: schema.graphql
documents: "**/*.ts"
extensions:
  extractConfig:
    magicComment: "MyGraphQL"
    tagIdentifiers: ["myGql", "graphql"]
    modules: ["my-graphql-lib"]
    allowGlobalIdentifiers: true
```

### Custom Lint Rules

```yaml
schema: schema.graphql
documents: "**/*.graphql"
extensions:
  project:
    lint:
      unique_names: error
      no_deprecated: warn
```

### Recommended Preset with Overrides

```yaml
schema: schema.graphql
documents: "**/*.graphql"
extensions:
  project:
    lint:
      recommended: error
      no_deprecated: off # Override to disable this rule
```

## Schema Features

The schema provides:

- **Validation**: Ensures required fields are present and values are correct types
- **Autocompletion**: Suggests available fields and values
- **Documentation**: Shows descriptions for each field on hover
- **Error Detection**: Highlights invalid configurations immediately

## Supported Fields

### Top Level

- `schema` (required): String or array of schema file paths/patterns/URLs
- `documents`: String or array of document file patterns
- `include`: String or array of file patterns to include
- `exclude`: String or array of file patterns to exclude
- `extensions`: Object containing tool-specific configuration
- `projects`: Object mapping project names to project configurations (for multi-project setups)

### Extensions

#### `extensions.extractConfig`

Configuration for extracting GraphQL from TypeScript/JavaScript files:

- `magicComment`: String to look for in comments (default: `"GraphQL"`)
- `tagIdentifiers`: Array of tag names to extract (default: `["gql", "graphql"]`)
- `modules`: Array of module names to recognize (default: graphql-tag, @apollo/client, etc.)
- `allowGlobalIdentifiers`: Boolean to allow extraction without imports (default: `false`)

#### `extensions.project.lint`

Linting configuration:

- String value `"recommended"` to use preset
- Object with rule configurations:
  - `recommended`: Severity to apply recommended rules
  - `unique_names`: Ensure operation/fragment names are unique
  - `no_deprecated`: Warn about deprecated field usage
  - Additional custom rules

Severity values: `"off"`, `"warn"`, `"error"`

## Publishing

To make the schema publicly available:

1. Commit the schema file to your repository
2. Use the raw GitHub URL in the `$schema` comment:

   ```yaml
   # yaml-language-server: $schema=https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/crates/graphql-config/schem./crates/graphql-config/schema/graphqlrc.schema.json
   ```

3. (Optional) Publish to [Schema Store](https://www.schemastore.org/) for automatic IDE support without manual configuration

## Updating the Schema

When adding new configuration options to the GraphQL LSP:

1. Update the Rust types in `crates/graphql-config/src/config.rs`
2. Update this JSON Schema to match
3. Add examples to this README
4. Update the schema version/date if needed
