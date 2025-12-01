# graphql-extract

A Rust library for extracting GraphQL queries, mutations, and fragments from TypeScript and JavaScript source files.

## Features

- **Tagged Template Literals**: Extract GraphQL from `gql` and `graphql` template tags
- **Source Location Tracking**: Precise position mapping between extracted GraphQL and original source
- **Import Validation**: Ensures GraphQL is only extracted from recognized modules
- **Custom Configuration**: Support for custom tag names and module sources
- **Multiple Patterns**: Handles various GraphQL embedding patterns in code

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
graphql-extract = { path = "../graphql-extract" }
```

## Getting Started

### Extract from a File

```rust
use graphql_extract::{extract_from_file, Language};

// Extract GraphQL from a TypeScript file
let result = extract_from_file("src/queries.ts", Language::TypeScript)?;

for extracted in result.documents {
    println!("Found GraphQL:");
    println!("{}", extracted.content);
    println!("At line {} col {}",
        extracted.location.start.line,
        extracted.location.start.column
    );
}
```

### Extract from a String

```rust
use graphql_extract::{extract_from_source, Language};

let source = r#"
const query = gql`
  query GetUser {
    user { id }
  }
`;
"#;

let result = extract_from_source(source, Language::TypeScript)?;
```

### Auto-Detect Language from File Extension

```rust
use graphql_extract::Language;

let lang = Language::from_path("file.tsx")?;
// lang == Language::TypeScript

let lang = Language::from_path("file.jsx")?;
// lang == Language::JavaScript
```

## Supported Patterns

### Basic Tagged Template Literal

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

### Multiple Queries in One File

```typescript
const query1 = gql`query A { user { id } }`;
const query2 = gql`query B { posts { title } }`;
```

### Fragments

```typescript
const userFragment = gql`
  fragment UserFields on User {
    id
    name
    email
  }
`;

const query = gql`
  query GetUser {
    user {
      ...UserFields
    }
  }
  ${userFragment}
`;
```

### Call Expressions with Arguments

```typescript
const fragment1 = graphql`fragment F1 on User { id }`;
const fragment2 = graphql`fragment F2 on User { name }`;

const document = graphql(`
  query GetUser {
    user {
      ...F1
      ...F2
    }
  }
`, [fragment1, fragment2]);
```

The extractor processes the first argument (the template literal) and ignores additional arguments.

## Configuration

### Custom Tag Names and Modules

```rust
use graphql_extract::ExtractConfig;

let config = ExtractConfig {
    magic_comment: "GraphQL".to_string(),
    tag_identifiers: vec!["gql".to_string(), "query".to_string()],
    modules: vec![
        "graphql-tag".to_string(),
        "@apollo/client".to_string(),
        "my-custom-module".to_string(),
    ],
    allow_global_identifiers: false,
};

let result = extract_from_file("src/queries.ts", &config)?;
```

### Default Configuration

By default, the extractor recognizes:

**Tag identifiers:**
- `gql`
- `graphql`

**Modules:**
- `graphql-tag`
- `@apollo/client`
- `apollo-server`
- `apollo-server-express`
- `gatsby`
- `react-relay`

## Import Tracking

By default, the extractor only extracts GraphQL from recognized module imports:

```typescript
// ✓ Will be extracted (gql imported from graphql-tag)
import { gql } from 'graphql-tag';
const query = gql`query { ... }`;

// ✗ Will NOT be extracted (gql imported from unknown module)
import { gql } from 'unknown-module';
const query = gql`query { ... }`;

// ✗ Will NOT be extracted by default (no import)
const query = gql`query { ... }`;
```

To allow extraction without imports (when `gql` is globally available):

```rust
let config = ExtractConfig {
    allow_global_identifiers: true,
    ..Default::default()
};
```

## Source Location Mapping

The library tracks precise source locations, mapping between the original TypeScript/JavaScript file and the extracted GraphQL content.

### ExtractedGraphQL Structure

```rust
pub struct ExtractedGraphQL {
    pub content: String,              // The extracted GraphQL
    pub location: SourceLocation,     // Location in original file
}

pub struct SourceLocation {
    pub start: Position,
    pub end: Position,
}

pub struct Position {
    pub line: usize,    // 1-based line number
    pub column: usize,  // 0-based column offset
}
```

This enables:
- **Accurate error reporting** at the correct line in the original file
- **Goto definition** navigation from code to GraphQL definitions
- **Hover information** showing type info in embedded GraphQL

## API Reference

### Functions

#### `extract_from_file(path: &str, language: Language) -> Result<ExtractionResult>`

Extracts GraphQL from a file using the default configuration.

#### `extract_from_source(source: &str, language: Language) -> Result<ExtractionResult>`

Extracts GraphQL from a source string using the default configuration.

#### `extract_from_file_with_config(path: &str, config: &ExtractConfig) -> Result<ExtractionResult>`

Extracts GraphQL from a file using a custom configuration.

#### `extract_from_source_with_config(source: &str, config: &ExtractConfig) -> Result<ExtractionResult>`

Extracts GraphQL from a source string using a custom configuration.

### Types

#### Language

```rust
pub enum Language {
    TypeScript,
    JavaScript,
}

impl Language {
    pub fn from_path(path: &str) -> Result<Self>;
}
```

File extensions:
- `.ts`, `.tsx` → TypeScript
- `.js`, `.jsx` → JavaScript

#### ExtractionResult

```rust
pub struct ExtractionResult {
    pub documents: Vec<ExtractedGraphQL>,
}
```

#### ExtractConfig

```rust
pub struct ExtractConfig {
    pub magic_comment: String,
    pub tag_identifiers: Vec<String>,
    pub modules: Vec<String>,
    pub allow_global_identifiers: bool,
}
```

## Examples

### Extract from Multiple Files

```rust
use graphql_extract::{extract_from_file, Language};
use std::path::Path;

fn extract_from_directory(dir: &Path) -> Result<Vec<String>> {
    let mut all_graphql = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();

        if let Ok(lang) = Language::from_path(path.to_str().unwrap()) {
            let result = extract_from_file(path.to_str().unwrap(), lang)?;

            for extracted in result.documents {
                all_graphql.push(extracted.content);
            }
        }
    }

    Ok(all_graphql)
}
```

### Custom Configuration for a Framework

```rust
use graphql_extract::{ExtractConfig, extract_from_file, Language};

// Configuration for Relay
let relay_config = ExtractConfig {
    tag_identifiers: vec!["graphql".to_string()],
    modules: vec!["react-relay".to_string()],
    allow_global_identifiers: true,  // Relay uses global graphql
    ..Default::default()
};

let result = extract_from_file("Component.tsx", &relay_config)?;
```

## Implementation Details

### Parser

Uses [SWC (Speedy Web Compiler)](https://swc.rs/) for parsing:
- Fast, production-ready TypeScript/JavaScript parser
- Full support for modern JavaScript features including JSX/TSX
- Accurate source location information

### AST Traversal

Traverses the SWC AST looking for:
- `TaggedTemplateExpression` nodes with matching tag names
- Import statements from recognized modules
- Template literals containing GraphQL

### Error Handling

Handles common edge cases:
- Malformed TypeScript/JavaScript (returns parse errors)
- Non-GraphQL tagged templates (silently ignored)
- Empty template literals (skipped)
- Interpolated values in templates (preserved as-is)

## Limitations

- Template literal interpolation is preserved but not evaluated
- Dynamic tag names are not supported (must be static identifiers)
- Minified code may have inaccurate source locations
- Only processes the first argument in call expressions

## License

MIT OR Apache-2.0
