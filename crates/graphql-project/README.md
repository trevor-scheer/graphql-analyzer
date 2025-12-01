# graphql-project

Core library for managing GraphQL projects with validation, indexing, and language service features.

## Features

- **Schema Loading**: From local files, glob patterns, or remote URLs (via introspection)
- **Document Loading**: Pure GraphQL files and embedded GraphQL in TypeScript/JavaScript
- **Project-Wide Validation**: Apollo compiler-based validation across all documents
- **Indexing**: Fast lookup structures for fragments, operations, types, and fields
- **Language Services**: Goto definition, find references, hover information
- **Incremental Updates**: Efficient document updates and re-validation
- **Concurrent Access**: Thread-safe operations using DashMap

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
graphql-project = { path = "../graphql-project" }
```

## Getting Started

### Create and Load a Project

```rust
use graphql_project::GraphQLProject;
use graphql_config::load_config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = load_config(".")?;
    let project_config = config.projects.get("default").unwrap();

    // Create project
    let project = GraphQLProject::new(project_config, ".").await?;

    // Validate all documents
    let diagnostics = project.validate_all().await?;

    for (file, diags) in diagnostics {
        for diag in diags {
            println!("{}: {}", file, diag.message);
        }
    }

    Ok(())
}
```

### Validate a Single Document

```rust
let diagnostics = project.validate_document("src/queries.graphql").await?;

for diag in diagnostics {
    println!("Line {}: {}", diag.location.line, diag.message);
}
```

### Update a Document

```rust
let new_content = r#"
    query GetUser($id: ID!) {
        user(id: $id) {
            id
            name
        }
    }
"#;

project.update_document("src/queries.graphql", new_content).await?;

// Re-validate after update
let diagnostics = project.validate_document("src/queries.graphql").await?;
```

## Schema Loading

### From Local Files

```rust
use graphql_project::SchemaLoader;

// Single file
let schema = SchemaLoader::load("schema.graphql").await?;

// Multiple files (glob pattern)
let schema = SchemaLoader::load("schema/**/*.graphql").await?;
```

### From Remote URL

```rust
// Automatically fetches via introspection
let schema = SchemaLoader::load("https://api.example.com/graphql").await?;
```

### Schema Sources

Supported schema sources:
- **Local file**: `schema.graphql`
- **Glob pattern**: `schema/**/*.graphql`
- **Remote URL**: `https://api.example.com/graphql` (fetched via introspection)
- **Multiple sources**: Automatically stitched together

## Document Loading

### Pure GraphQL Files

```graphql
# queries.graphql
query GetUser($id: ID!) {
    user(id: $id) {
        id
        name
    }
}
```

### Embedded in TypeScript/JavaScript

```typescript
import { gql } from 'graphql-tag';

const query = gql`
    query GetUser($id: ID!) {
        user(id: $id) {
            id
            name
        }
    }
`;
```

The library automatically:
- Extracts GraphQL from tagged template literals
- Tracks position mappings for accurate diagnostics
- Adjusts positions for goto definition and hover

## Indexing

The library builds fast lookup structures for:

### Document Index

```rust
use graphql_project::DocumentIndex;

// Get fragment definition
let fragment = document_index.get_fragment("UserFields")?;
println!("Fragment at {}:{}", fragment.location.line, fragment.location.column);

// Get operation definition
let operation = document_index.get_operation("GetUser")?;
println!("Operation at {}:{}", operation.location.line, operation.location.column);
```

### Schema Index

```rust
use graphql_project::SchemaIndex;

// Get type definition
let type_def = schema_index.get_type("User")?;
println!("Type: {}", type_def.name);

// Get field definition
let field = schema_index.get_field("User", "name")?;
println!("Field type: {}", field.type_name);

// Get directive definition
let directive = schema_index.get_directive("deprecated")?;
println!("Directive: {}", directive.name);
```

## Language Services

### Goto Definition

Navigate to definitions for GraphQL elements:

```rust
let definition = project.goto_definition(
    "src/queries.graphql",
    5,  // line
    10  // column
).await?;

if let Some(def) = definition {
    println!("Definition at {}:{}:{}", def.uri, def.range.start.line, def.range.start.character);
}
```

Supports:
- Fragment spreads → fragment definitions
- Operation names → operation definitions
- Type references → type definitions
- Field references → schema field definitions
- Variable references → operation variable definitions
- Argument names → schema argument definitions
- Enum values → enum value definitions
- Directive names and arguments

### Find References

Find all usages of GraphQL elements:

```rust
let references = project.find_references(
    "src/schema.graphql",
    10,  // line at fragment definition
    5    // column
).await?;

for reference in references {
    println!("Used at {}:{}:{}", reference.uri, reference.range.start.line, reference.range.start.character);
}
```

Supports:
- Fragment definitions → all fragment spreads
- Type definitions → all usages in field types, union members, implements clauses

### Hover Information

Get type information and descriptions:

```rust
let hover = project.hover(
    "src/queries.graphql",
    5,  // line
    10  // column
).await?;

if let Some(info) = hover {
    println!("{}", info.contents);
}
```

Shows:
- Type information for fields
- Descriptions from schema
- Deprecation warnings

## Validation

### Apollo Compiler Validation

The library uses apollo-compiler for GraphQL spec-compliant validation:

```rust
// Validate all documents
let all_diagnostics = project.validate_all().await?;

// Validate specific document
let doc_diagnostics = project.validate_document("src/queries.graphql").await?;
```

Checks:
- Schema validity
- Document syntax
- Type correctness
- Field existence
- Argument validity
- Fragment usage
- Variable definitions

## API Reference

### GraphQLProject

```rust
impl GraphQLProject {
    pub async fn new(
        config: &ProjectConfig,
        root_path: &str
    ) -> Result<Self>;

    pub async fn validate_all(&self) -> Result<HashMap<String, Vec<Diagnostic>>>;

    pub async fn validate_document(
        &self,
        uri: &str
    ) -> Result<Vec<Diagnostic>>;

    pub async fn update_document(
        &self,
        uri: &str,
        content: String
    ) -> Result<()>;

    pub async fn goto_definition(
        &self,
        uri: &str,
        line: usize,
        column: usize
    ) -> Result<Option<Location>>;

    pub async fn find_references(
        &self,
        uri: &str,
        line: usize,
        column: usize
    ) -> Result<Vec<Location>>;

    pub async fn hover(
        &self,
        uri: &str,
        line: usize,
        column: usize
    ) -> Result<Option<HoverInfo>>;
}
```

### SchemaLoader

```rust
impl SchemaLoader {
    pub async fn load(source: &str) -> Result<Schema>;

    pub async fn load_remote(url: &str) -> Result<String>;

    pub async fn load_files(patterns: &[String]) -> Result<String>;
}
```

### DocumentLoader

```rust
impl DocumentLoader {
    pub fn load(path: &str) -> Result<Document>;

    pub fn load_all(patterns: &[String]) -> Result<Vec<Document>>;
}
```

### Core Types

#### Diagnostic

```rust
pub struct Diagnostic {
    pub message: String,
    pub severity: Severity,
    pub location: Location,
}

pub struct Location {
    pub uri: String,
    pub range: Range,
}

pub struct Range {
    pub start: Position,
    pub end: Position,
}

pub struct Position {
    pub line: usize,
    pub character: usize,
}
```

#### DocumentIndex

```rust
pub struct DocumentIndex {
    fragments: HashMap<String, FragmentDefinition>,
    operations: HashMap<String, OperationDefinition>,
}
```

#### SchemaIndex

```rust
pub struct SchemaIndex {
    types: HashMap<String, TypeDefinition>,
    directives: HashMap<String, DirectiveDefinition>,
}
```

## Examples

### Watch Mode Implementation

```rust
use graphql_project::GraphQLProject;
use notify::{Watcher, RecursiveMode};

let project = GraphQLProject::new(&config, ".").await?;

let mut watcher = notify::watcher(tx, Duration::from_secs(1))?;
watcher.watch("src", RecursiveMode::Recursive)?;

loop {
    match rx.recv() {
        Ok(event) => {
            if let Some(path) = event.path {
                let content = std::fs::read_to_string(&path)?;
                project.update_document(path.to_str().unwrap(), content).await?;

                let diagnostics = project.validate_document(path.to_str().unwrap()).await?;
                for diag in diagnostics {
                    println!("{}: {}", path.display(), diag.message);
                }
            }
        }
        Err(e) => println!("Watch error: {:?}", e),
    }
}
```

### Custom Validation Pipeline

```rust
use graphql_project::GraphQLProject;

let project = GraphQLProject::new(&config, ".").await?;

// Apollo compiler validation
let apollo_diagnostics = project.validate_all().await?;

// Custom validation (use graphql-linter)
use graphql_linter::{Linter, ProjectContext};

let linter = Linter::new(lint_config);
let ctx = ProjectContext {
    documents: &project.document_index,
    schema: &project.schema_index,
};

let lint_diagnostics = linter.lint_project(&ctx);

// Merge diagnostics
for (file, diags) in apollo_diagnostics {
    println!("{}:", file);
    for diag in diags {
        println!("  [apollo] {}", diag.message);
    }
}

for (file, diags) in lint_diagnostics {
    println!("{}:", file);
    for diag in diags {
        println!("  [lint] {}", diag.message);
    }
}
```

### Multi-Project Management

```rust
use graphql_project::GraphQLProject;
use graphql_config::load_config;
use std::collections::HashMap;

let config = load_config(".")?;
let mut projects = HashMap::new();

for (name, project_config) in &config.projects {
    let project = GraphQLProject::new(project_config, ".").await?;
    projects.insert(name.clone(), project);
}

// Validate all projects
for (name, project) in &projects {
    println!("Validating project: {}", name);
    let diagnostics = project.validate_all().await?;

    for (file, diags) in diagnostics {
        for diag in diags {
            println!("  {}: {}", file, diag.message);
        }
    }
}
```

## Implementation Details

### Parser

Uses [apollo-compiler](https://docs.rs/apollo-compiler/) for:
- Accurate error messages
- Full GraphQL spec compliance
- Built-in validation rules
- Schema stitching

### Position Mapping

For TypeScript/JavaScript files:
1. Extract GraphQL using `graphql-extract`
2. Track position mappings between extracted and original source
3. Translate positions for accurate diagnostics and goto definition

### Concurrency

Uses [DashMap](https://docs.rs/dashmap/) for concurrent access:
- Multiple LSP requests handled in parallel
- Safe updates from file watchers
- Lock-free reads for common operations

### Caching

- Schema parsed and cached on load
- Documents cached until updated
- Indices rebuilt incrementally on document changes

## Performance Considerations

### Document Updates

Only affected documents are re-validated:

```rust
// Fast: Only validates the changed document
project.update_document("queries.graphql", new_content).await?;
let diags = project.validate_document("queries.graphql").await?;

// Slower: Validates all documents
let all_diags = project.validate_all().await?;
```

### Index Queries

Index lookups are O(1) hash map operations:

```rust
// Fast: Direct lookup
let fragment = document_index.get_fragment("UserFields")?;

// Slow: Linear scan (avoid)
for (name, fragment) in &document_index.fragments {
    if name.starts_with("User") {
        // ...
    }
}
```

## License

MIT OR Apache-2.0
