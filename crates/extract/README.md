# graphql-extract

A Rust library for extracting GraphQL queries, mutations, and fragments from TypeScript, JavaScript, Vue, Svelte, and Astro source files.

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
use graphql_extract::{extract_from_source, ExtractConfig, Language};

let source = r#"
const query = gql`
  query GetUser {
    user { id }
  }
`;
"#;

let config = ExtractConfig::default();
let result = extract_from_source(source, Language::TypeScript, &config, "file.ts")?;
```

### Auto-Detect Language from File Extension

```rust
use graphql_extract::Language;

let lang = Language::from_path("file.tsx")?;
// lang == Language::TypeScript

let lang = Language::from_path("file.jsx")?;
// lang == Language::JavaScript

let lang = Language::from_path("component.vue")?;
// lang == Language::Vue
```

## Supported Patterns

### Basic Tagged Template Literal

```typescript
import { gql } from "graphql-tag";

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
const query1 = gql`
  query A {
    user {
      id
    }
  }
`;
const query2 = gql`
  query B {
    posts {
      title
    }
  }
`;
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
const fragment1 = graphql`
  fragment F1 on User {
    id
  }
`;
const fragment2 = graphql`
  fragment F2 on User {
    name
  }
`;

const document = graphql(
  `
    query GetUser {
      user {
        ...F1
        ...F2
      }
    }
  `,
  [fragment1, fragment2],
);
```

The extractor processes the first argument (the template literal) and ignores additional arguments.

## Framework Support

### Vue Single File Components

Extracts GraphQL from `<script>` and `<script setup>` blocks in `.vue` files, including `lang="ts"` variants.

```vue
<script setup lang="ts">
import { gql } from "graphql-tag";

const query = gql`
  query GetUser {
    user {
      id
      name
    }
  }
`;
</script>
```

### Svelte Components

Extracts GraphQL from `<script>` blocks in `.svelte` files, including `lang="ts"` and `context="module"` variants.

### Astro Pages

Extracts GraphQL from the frontmatter (`---` fenced) section of `.astro` files.

All three frameworks work by extracting `<script>` blocks (or frontmatter) and delegating to the existing TypeScript/JavaScript extraction pipeline. This means all the same tagged template patterns, import tracking, and configuration options apply within framework files.

## Configuration

The `ExtractConfig` schema mirrors `@graphql-tools/graphql-tag-pluck` so configs are portable between this crate and the JS/TS ecosystem.

### Custom Modules and Identifiers

```rust
use graphql_extract::{ExtractConfig, ModuleConfig};

let config = ExtractConfig {
    gql_magic_comment: "graphql".to_string(),
    modules: vec![
        ModuleConfig { name: "graphql-tag".to_string(), identifier: None },
        ModuleConfig {
            name: "@apollo/client".to_string(),
            identifier: Some("gql".to_string()),
        },
        ModuleConfig {
            name: "my-custom-module".to_string(),
            identifier: Some("gql".to_string()),
        },
    ],
    global_gql_identifier_name: vec!["gql".to_string(), "graphql".to_string()],
    gql_vue_block: None,
    skip_indent: false,
};

let result = extract_from_file("src/queries.ts", &config)?;
```

### Default Configuration

Defaults follow `@graphql-tools/graphql-tag-pluck` (minus the legacy unscoped `apollo-*` packages — modern Apollo lives at `@apollo/client(/core)`).

**Bare/global identifiers (`globalGqlIdentifierName`):** `gql`, `graphql` — recognized as GraphQL tags without an import. Set to an empty list (or `false` in JSON) to require imports for every tag.

**Modules:** `graphql-tag`, `graphql-tag.macro`, `@apollo/client`, `@apollo/client/core`, `gatsby`, `react-relay`, `react-relay/hooks`, `relay-runtime`, `babel-plugin-relay/macro`, `graphql.macro`, `urql`, `@urql/core`, `@urql/preact`, `@urql/svelte`, `@urql/vue`. Modules with an `identifier` constraint only recognize the named import matching that identifier.

## Import Tracking

The extractor accepts a tag if its local binding is either tracked from a recognized module import or matches `globalGqlIdentifierName`:

```typescript
// ✓ `gql` is in globalGqlIdentifierName by default — extracted
import { gql } from "graphql-tag";
const query = gql`query { ... }`;

// ✓ `gql` matches @apollo/client's identifier constraint — extracted
import { gql } from "@apollo/client";
const query = gql`query { ... }`;

// ✓ Bare `gql` matches globalGqlIdentifierName — extracted by default
const query = gql`query { ... }`;

// ✗ Tag name is not in globals and no recognized import — not extracted
const query = mytag`query { ... }`;
```

To require imports for every tag (no bare/global extraction):

```rust
let config = ExtractConfig {
    global_gql_identifier_name: Vec::new(),
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

#### `extract_from_source(source: &str, language: Language, config: &ExtractConfig, path: &str) -> Result<ExtractionResult>`

Extracts GraphQL from a source string. The `path` is used in error messages and SWC diagnostics.

### Types

#### Language

```rust
pub enum Language {
    TypeScript,
    JavaScript,
    Vue,
    Svelte,
    Astro,
}

impl Language {
    pub fn from_path(path: &str) -> Result<Self>;
}
```

File extensions:

- `.ts`, `.tsx` → TypeScript
- `.js`, `.jsx` → JavaScript
- `.vue` → Vue
- `.svelte` → Svelte
- `.astro` → Astro

#### ExtractionResult

```rust
pub struct ExtractionResult {
    pub documents: Vec<ExtractedGraphQL>,
}
```

#### ExtractConfig

```rust
pub struct ExtractConfig {
    pub modules: Vec<ModuleConfig>,
    pub gql_magic_comment: String,
    pub global_gql_identifier_name: Vec<String>,
    pub gql_vue_block: Option<String>,
    pub skip_indent: bool,
}

pub struct ModuleConfig {
    pub name: String,
    pub identifier: Option<String>,
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
use graphql_extract::{ExtractConfig, ModuleConfig, extract_from_file};

// Configuration for Relay (matches pluck's defaults for these modules)
let relay_config = ExtractConfig {
    modules: vec![
        ModuleConfig {
            name: "react-relay".to_string(),
            identifier: Some("graphql".to_string()),
        },
        ModuleConfig {
            name: "relay-runtime".to_string(),
            identifier: Some("graphql".to_string()),
        },
    ],
    global_gql_identifier_name: vec!["graphql".to_string()],
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
