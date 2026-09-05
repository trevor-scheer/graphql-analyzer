# graphql-config

A Rust library for parsing and discovering GraphQL configuration files, compatible with the standard `.graphqlrc` format used by popular GraphQL tools.

## Features

- **Multiple Formats**: Supports YAML, JSON, and TOML configuration files
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

```toml
# .graphqlrc.toml
schema = "schema.graphql"
documents = "src/**/*.graphql"
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

```toml
# .graphqlrc.toml
[projects.api]
schema = "api/schema.graphql"
documents = "api/**/*.graphql"

[projects.client]
schema = ["client/schema.graphql", "client/schema/*.graphql"]
documents = ["client/**/*.graphql", "client/**/*.tsx"]
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

### Resolved Schema

When your build pipeline transforms the schema (e.g. directive-based transforms), you can point to the build output so queries validate against the final schema:

```yaml
extensions:
  graphql-analyzer:
    resolvedSchema: "generated/schema.graphql"
```

Source schema files are still used for goto-definition and hover. The resolved schema is used for query validation and completions. SDL validation on source files is skipped since they may be intentionally incomplete.

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

Parses configuration from a string. Format should be "yaml", "json", or "toml".

#### `has_remote_schema(config: &ProjectConfig) -> bool`

Checks if a project configuration uses a remote URL for its schema.

## Supported Configuration Files

The library searches for these files in order of preference:

1. `.graphqlrc.yml`
2. `.graphqlrc.yaml`
3. `.graphqlrc.json`
4. `.graphqlrc.toml`
5. `.graphqlrc` (YAML or JSON, auto-detected)
6. `graphql.config.yml`
7. `graphql.config.yaml`
8. `graphql.config.json`
9. `graphql.config.toml`

### Note on JavaScript/TypeScript Configs

This library only supports YAML, JSON, and TOML configuration formats. JavaScript and TypeScript config files (`graphql.config.js`, `graphql.config.ts`) are **not supported**.

If your JS/TS config is a static object (or evaluates to one), most configurations translate directly:

```javascript
// graphql.config.js (NOT SUPPORTED)
module.exports = {
  schema: "schema.graphql",
  documents: "src/**/*.graphql",
};
```

```yaml
# .graphqlrc.yml (equivalent)
schema: schema.graphql
documents: src/**/*.graphql
```

```toml
# .graphqlrc.toml (equivalent)
schema = "schema.graphql"
documents = "src/**/*.graphql"
```

#### One-liner migration

For static configs, dump to JSON in one command — `.graphqlrc.json` is supported directly:

```sh
node -e "import('./graphql.config.js').then(c => console.log(JSON.stringify(c.default ?? c, null, 2)))" > .graphqlrc.json
```

The dynamic `import()` returns a promise resolving to the namespace object, which `.then(c => …)` unwraps. `c.default ?? c` picks the ESM default export when present, otherwise falls back to the CJS exports object — so the same command handles both `module.exports = …` and `export default …`.

For TypeScript configs, point at `graphql.config.ts` instead. Modern Node strips type annotations natively (Node 22.6+ with `--experimental-strip-types`, or Node 23.6+ where it's on by default), so no separate runner like `tsx` is needed.

#### What won't carry over

`graphql.config.js` is loaded by [cosmiconfig](https://github.com/cosmiconfig/cosmiconfig), which evaluates JavaScript at load time. The following are **not supported** in static configs:

- **Function-valued fields anywhere in the tree** — common cases include HTTP `Authorization` headers built by a function for dynamic auth tokens, or `extensions.codegen.hooks.afterAllFileWrite` set to a function (only string shell commands carry over). Function values silently disappear from `JSON.stringify()`, so the one-liner produces an _incomplete_ config rather than erroring.
- **`process.env` interpolation via JS** — e.g. `headers.Authorization` built from a template literal embedding `process.env.TOKEN`. Use the static `${TOKEN}` interpolation supported by this library instead.
- **Conditional branches on runtime state** — e.g. switching `documents` globs or `projects` based on `process.env.CI` or `NODE_ENV`.
- **Spread-imported config fragments** — e.g. spreading the result of `require("./shared-globs")` into the `documents` array.
- **Custom loaders, transforms, or plugins registered in JS** — e.g. graphql-config v5's top-level `loaders: [new UrlLoader()]`, or codegen plugins referenced via `require()`.

For dynamic needs, use environment variable interpolation (`${VAR}` and `${VAR:default}`) or generate the config file as a build step before invoking the tool.

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
- Invalid YAML/JSON/TOML syntax (`ConfigError::ParseError`)
- Invalid configuration structure (`ConfigError::ValidationError`)
- Missing required fields

## License

MIT OR Apache-2.0
