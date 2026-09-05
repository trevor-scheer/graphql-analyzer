# GraphQL Configuration Schema

This directory contains a JSON Schema for `.graphqlrc` configuration files used by GraphQL LSP and related tools.

## What is it?

The [graphqlrc.schema.json](./graphqlrc.schema.json) file provides IDE validation, autocompletion, and documentation for GraphQL configuration files.

## How to Use

### VS Code with GraphQL Analyzer Extension

If you have the GraphQL Analyzer VS Code extension installed, schema validation is automatic - no configuration needed.

### Manual Configuration

#### VS Code

Add a comment at the top of your `.graphqlrc.yaml` or `.graphqlrc.yml` file:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/crates/config/schema/graphqlrc.schema.json
schema: schema.graphql
documents: "**/*.{graphql,gql,ts,tsx}"
```

Or configure it globally in your VS Code settings (`.vscode/settings.json`):

```json
{
  "yaml.schemas": {
    "https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/crates/config/schema/graphqlrc.schema.json": [
      ".graphqlrc.yaml",
      ".graphqlrc.yml",
      "graphql.config.yaml",
      "graphql.config.yml"
    ]
  },
  "json.schemas": [
    {
      "fileMatch": [".graphqlrc.json", ".graphqlrc"],
      "url": "https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/crates/config/schema/graphqlrc.schema.json"
    }
  ]
}
```

#### JetBrains IDEs (IntelliJ, WebStorm, etc.)

1. Open Settings -> Languages & Frameworks -> Schemas and DTDs -> JSON Schema Mappings
2. Add a new mapping:
   - Name: GraphQL Configuration
   - Schema file or URL: `https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/crates/config/schema/graphqlrc.schema.json`
   - Schema version: JSON Schema version 7
   - File path pattern: `.graphqlrc*`, `graphql.config.*`

## Configuration Examples

### Single Project

```yaml
schema: schema.graphql
documents: "**/*.{graphql,gql,ts,tsx,js,jsx}"
extensions:
  graphql-analyzer:
    client: apollo
    lint: recommended
```

### Multi-Project

```yaml
projects:
  api:
    schema: api/schema.graphql
    documents: "api/**/*.graphql"
    extensions:
      graphql-analyzer:
        client: none
        lint:
          extends: recommended
          rules:
            noDeprecated: off

  client:
    schema: client/schema.graphql
    documents: "client/**/*.{ts,tsx}"
    extensions:
      graphql-analyzer:
        client: apollo
        extractConfig:
          modules:
            - { name: "@apollo/client", identifier: "gql" }
          globalGqlIdentifierName: ["gql"]
        lint: recommended
```

### Custom Extract Configuration

```yaml
schema: schema.graphql
documents: "**/*.ts"
extensions:
  graphql-analyzer:
    client: apollo
    extractConfig:
      gqlMagicComment: "MyGraphQL"
      modules:
        - { name: "my-graphql-lib", identifier: "myGql" }
      globalGqlIdentifierName: ["myGql", "graphql"]
```

### Migrating from `@graphql-tools/graphql-tag-pluck`

The `extractConfig` schema mirrors pluck's. Users coming from
`@graphql-eslint` or any pluck-based pipeline can paste their pluck config
under the `pluckConfig` key (alias for `extractConfig`):

```yaml
schema: schema.graphql
documents: "**/*.ts"
extensions:
  graphql-analyzer:
    pluckConfig:
      modules:
        - graphql-tag
        - { name: "@apollo/client", identifier: gql }
      globalGqlIdentifierName: ["gql", "graphql"]
```

> **Note:** Setting both `extractConfig` and `pluckConfig` on the same project
> is a configuration error — they are aliases.

### Lint Rules with Options

```yaml
schema: schema.graphql
documents: "**/*.graphql"
extensions:
  graphql-analyzer:
    client: none
    lint:
      extends: recommended
      rules:
        uniqueNames: error
        noDeprecated: warn
        # ESLint-style array format with options
        requireSelections: [warn, { fieldName: ["id", "nodeId"], requireAllFields: true }]
```

### Preset with Overrides

```yaml
schema: schema.graphql
documents: "**/*.graphql"
extensions:
  graphql-analyzer:
    client: none
    lint:
      extends: recommended
      rules:
        noDeprecated: off # Override to disable this rule
```

## Schema Features

The schema provides:

- **Validation**: Ensures required fields are present and values are correct types
- **Autocompletion**: Suggests available fields, lint rules, and values
- **Documentation**: Shows descriptions for each field on hover
- **Error Detection**: Highlights invalid configurations immediately

## Supported Fields

### Top Level

- `schema` (required): String or array of schema file paths/patterns/URLs
- `documents`: String or array of document file patterns
- `include`: String or array of file patterns to include
- `exclude`: String or array of file patterns to exclude
- `extensions`: Object containing tool-specific configuration (namespaced by tool)
- `projects`: Object mapping project names to project configurations (for multi-project setups)

### Extensions

All graphql-analyzer extensions are namespaced under `extensions.graphql-analyzer`.

#### `extensions.graphql-analyzer.client`

Specifies the GraphQL client library used in the project. This determines which client-side directives are available for validation.

Values: `apollo`, `relay`, `none`

| Value    | Directives                                                                    |
| -------- | ----------------------------------------------------------------------------- |
| `apollo` | `@client`, `@connection`, `@defer`, `@export`, `@nonreactive`, `@unmask`      |
| `relay`  | `@arguments`, `@argumentDefinitions`, `@connection`, `@refetchable`, and more |
| `none`   | No client directives (server-only validation)                                 |

```yaml
extensions:
  graphql-analyzer:
    client: apollo
```

#### `extensions.graphql-analyzer.lint`

Linting configuration. Can be:

- String preset: `lint: recommended`
- Array of presets: `lint: [recommended]`
- Full configuration object:

```yaml
lint:
  extends: recommended # optional preset to extend
  rules:
    noDeprecated: warn
    uniqueNames: error
```

Available lint rules (use camelCase in config):

| Rule                    | Description                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| `noDeprecated`          | Warn about usage of deprecated fields and enum values             |
| `noAnonymousOperations` | Require all operations to have names                              |
| `uniqueNames`           | Ensure operation and fragment names are unique across the project |
| `noUnusedFragments`     | Warn about fragments that are defined but never used              |
| `noUnusedFields`        | Warn about fields that are selected but unused                    |
| `redundantFields`       | Warn about redundant field selections                             |
| `noUnusedVariables`     | Warn about variables that are declared but never used             |
| `operationNameSuffix`   | Require operation names to have a specific suffix                 |

Severity values: `off`, `warn`, `error`

Rule configuration formats:

```yaml
# Simple severity
noDeprecated: warn

# Object with options
requireSelections:
  severity: warn
  options:
    fieldName: ["id", "nodeId"]
    requireAllFields: true

# ESLint-style array
requireSelections: [warn, { fieldName: ["id", "nodeId"], requireAllFields: true }]
```

#### `extensions.graphql-analyzer.extractConfig` (alias: `pluckConfig`)

Configuration for extracting GraphQL from TypeScript/JavaScript files. Schema mirrors `@graphql-tools/graphql-tag-pluck` so configs are portable between tools.

- `modules`: Modules whose imports of GraphQL tags are recognized. Each entry is either a string (shorthand for `{ name }`) or `{ name, identifier? }`. Default: graphql-tag, graphql-tag.macro, @apollo/client, @apollo/client/core, gatsby, react-relay (and hooks/runtime variants), babel-plugin-relay/macro, graphql.macro, urql, @urql/{core,preact,svelte,vue}.
- `gqlMagicComment`: Magic comment string for `/* graphql */` style (default: `"graphql"`).
- `globalGqlIdentifierName`: Identifiers recognized as GraphQL tags without an import. Accepts a string, an array of strings, or `false` to disable (default: `["gql", "graphql"]`).
- `gqlVueBlock`: Optional Vue SFC block name (e.g. `"graphql"`) for raw GraphQL in custom blocks.
- `skipIndent`: If true, normalize indentation by stripping common leading whitespace from each line (default: `false`).

#### `extensions.graphql-analyzer.resolvedSchema`

Path to a resolved/final schema file. When set, queries are validated against this schema instead of the source schema files. Source files are still used for go-to-definition navigation.

This is useful when your build pipeline transforms the schema (e.g. directive-based transforms) and the source SDL doesn't match the runtime schema.

## Updating the Schema

When adding new configuration options:

1. Update the Rust types in `crates/config/src/config.rs`
2. Update this JSON Schema to match
3. Run tests to verify sync: `cargo test -p graphql-config schema_sync`

When adding new lint rules:

1. Add the rule to the linter registry
2. Add the rule to the schema's `FullLintConfig.properties.rules.properties`
3. Run tests to verify sync: `cargo test -p graphql-linter test_schema_includes_all_rules`

The sync tests will fail if the schema gets out of sync with the Rust types or lint rules.
