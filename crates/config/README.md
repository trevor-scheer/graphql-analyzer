# graphql-config

A Rust library for parsing and discovering GraphQL configuration files, compatible with the standard `.graphqlrc` format used by popular GraphQL tools.

## Features

- **Multiple Formats**: Supports YAML and JSON configuration files
- **Auto-Discovery**: Walks up the directory tree to find configuration files
- **Multi-Project Support**: Single or multiple GraphQL projects in one configuration
- **Glob Patterns**: Resolves glob patterns for schema and document files
- **Remote Schemas**: Detects URLs for remote schema introspection
- **Standard Format**: Compatible with GraphQL Code Generator, GraphQL ESLint, and other tools

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
graphql-config = { path = "../graphql-config" }
```

## Getting Started

### Load Configuration from a Directory

```rust
use graphql_config::load_config;

// Discovers and loads the nearest .graphqlrc file
let config = load_config("/path/to/project")?;

// Access projects
for (name, project) in &config.projects {
    println!("Project: {}", name);
    println!("  Schema: {:?}", project.schema);
    println!("  Documents: {:?}", project.documents);
}
```

### Find Configuration File Path

```rust
use graphql_config::find_config;

// Search for config file starting from a directory
let config_path = find_config("/path/to/project")?;
println!("Found config at: {}", config_path.display());
```

### Parse Configuration from String

```rust
use graphql_config::load_config_from_str;

let yaml = r#"
schema: schema.graphql
documents: "**/*.graphql"
"#;

let config = load_config_from_str(yaml, "yaml")?;
```

## Configuration Format

### Single Project

```yaml
# .graphqlrc.yml
schema: schema.graphql
documents: src/**/*.graphql
```

### Multiple Projects

```yaml
# .graphqlrc.yml
projects:
  api:
    schema: api/schema.graphql
    documents: api/**/*.graphql
  client:
    schema:
      - client/schema.graphql
      - client/schema/*.graphql
    documents:
      - client/**/*.graphql
      - client/**/*.tsx
```

### Schema Sources

Schemas can be loaded from multiple sources:

**Local files:**

```yaml
schema: schema.graphql
```

**Glob patterns:**

```yaml
schema: schema/**/*.graphql
```

**Multiple sources:**

```yaml
schema:
  - schema.graphql
  - extensions/*.graphql
```

**Remote URLs:**

```yaml
schema: https://api.example.com/graphql
```

Note: This library only parses and validates the configuration structure. Actual schema loading (including introspection for URLs) is handled by consumers of this library.

### Document Patterns

Documents can include GraphQL files and files with embedded GraphQL:

```yaml
documents:
  - "**/*.graphql"
  - "**/*.gql"
  - "**/*.tsx"
  - "**/*.ts"
```

## API Reference

### Core Types

#### GraphQLConfig

The top-level configuration structure:

```rust
pub struct GraphQLConfig {
    pub projects: HashMap<String, ProjectConfig>,
}
```

For single-project configs, there's an implicit "default" project.

#### ProjectConfig

Configuration for a single GraphQL project:

```rust
pub struct ProjectConfig {
    pub schema: SchemaConfig,
    pub documents: Option<DocumentsConfig>,
    // ... other fields
}
```

#### SchemaConfig

Schema source configuration:

```rust
pub enum SchemaConfig {
    File(String),
    Files(Vec<String>),
    Url(String),
}
```

#### DocumentsConfig

Document pattern configuration:

```rust
pub struct DocumentsConfig {
    pub patterns: Vec<String>,
}
```

### Key Functions

#### `load_config(path: &Path) -> Result<GraphQLConfig>`

Discovers and loads configuration from a directory, searching up the tree for config files.

#### `find_config(path: &Path) -> Result<PathBuf>`

Finds the path to the nearest configuration file without loading it.

#### `load_config_from_str(content: &str, format: &str) -> Result<GraphQLConfig>`

Parses configuration from a string. Format should be "yaml" or "json".

#### `has_remote_schema(config: &ProjectConfig) -> bool`

Checks if a project configuration uses a remote URL for its schema.

## Supported Configuration Files

The library searches for these files in order:

1. `.graphqlrc` (YAML or JSON)
2. `.graphqlrc.yml`
3. `.graphqlrc.yaml`
4. `.graphqlrc.json`

Future support planned for:

- `graphql.config.js`
- `graphql.config.ts`
- `graphql` section in `package.json`

## Examples

### Working with Multi-Project Configs

```rust
use graphql_config::load_config;

let config = load_config(".")?;

// Get a specific project
if let Some(api_project) = config.projects.get("api") {
    println!("API schema: {:?}", api_project.schema);
}

// Iterate all projects
for (name, project) in &config.projects {
    println!("Project '{}' documents: {:?}", name, project.documents);
}
```

### Checking for Remote Schemas

```rust
use graphql_config::{load_config, has_remote_schema};

let config = load_config(".")?;

for (name, project) in &config.projects {
    if has_remote_schema(project) {
        println!("Project '{}' uses a remote schema", name);
    }
}
```

### Error Handling

```rust
use graphql_config::{load_config, ConfigError};

match load_config(".") {
    Ok(config) => println!("Loaded {} projects", config.projects.len()),
    Err(ConfigError::NotFound) => eprintln!("No config file found"),
    Err(ConfigError::ParseError(msg)) => eprintln!("Parse error: {}", msg),
    Err(e) => eprintln!("Error: {}", e),
}
```

## Implementation Details

### File Discovery

Uses `walkdir` to recursively search for configuration files, starting from the specified directory and walking up to parent directories until a config file is found or the root is reached.

### Glob Pattern Resolution

Uses the `glob` crate to resolve file patterns. Patterns are resolved relative to the configuration file location.

### Error Handling

Provides detailed error types for:

- Missing configuration files (`ConfigError::NotFound`)
- Invalid YAML/JSON syntax (`ConfigError::ParseError`)
- Invalid configuration structure (`ConfigError::ValidationError`)
- Missing required fields

## License

MIT OR Apache-2.0
