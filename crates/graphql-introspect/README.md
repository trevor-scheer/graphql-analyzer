# graphql-introspect

A Rust library for fetching GraphQL schemas from remote endpoints via introspection and converting them to Schema Definition Language (SDL).

## Features

- **Standard Introspection**: Execute the standard GraphQL introspection query
- **Type-Safe**: Strongly typed introspection response structures
- **SDL Conversion**: Convert introspection JSON to clean, readable SDL
- **Smart Filtering**: Automatically filters built-in scalars, types, and directives
- **Complete Type Support**: Handles all GraphQL schema types and directives
- **Async**: Built on tokio and reqwest for async HTTP requests

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
graphql-introspect = { path = "../graphql-introspect" }
tokio = { version = "1", features = ["full"] }
```

## Getting Started

### One-Step Introspection to SDL

The simplest way to use this library:

```rust
use graphql_introspect::introspect_url_to_sdl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdl = introspect_url_to_sdl("https://api.example.com/graphql").await?;
    println!("{}", sdl);
    Ok(())
}
```

### Step-by-Step Usage

For more control over the introspection process:

```rust
use graphql_introspect::{execute_introspection, introspection_to_sdl};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Execute introspection query
    let introspection = execute_introspection("https://api.example.com/graphql").await?;

    // Convert to SDL
    let sdl = introspection_to_sdl(&introspection);

    println!("{}", sdl);
    Ok(())
}
```

### Working with Introspection Data

Access the raw introspection data structures:

```rust
use graphql_introspect::{execute_introspection, IntrospectionType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let introspection = execute_introspection("https://api.example.com/graphql").await?;

    // Access schema information
    let schema = &introspection.data.schema;

    // Iterate over types
    for type_def in &schema.types {
        match type_def {
            IntrospectionType::Object(obj) => {
                println!("Object type: {}", obj.name);
                for field in &obj.fields {
                    println!("  Field: {} -> {}", field.name, field.type_ref.to_string());
                }
            }
            IntrospectionType::Enum(enum_type) => {
                println!("Enum type: {}", enum_type.name);
                for value in &enum_type.enum_values {
                    println!("  Value: {}", value.name);
                }
            }
            _ => {}
        }
    }

    Ok(())
}
```

## API Reference

### Functions

#### `introspect_url_to_sdl(url: &str) -> Result<String, IntrospectionError>`

One-step function that executes introspection and converts to SDL.

```rust
let sdl = introspect_url_to_sdl("https://api.example.com/graphql").await?;
```

#### `execute_introspection(url: &str) -> Result<IntrospectionResponse, IntrospectionError>`

Executes the introspection query and returns the typed response.

```rust
let response = execute_introspection("https://api.example.com/graphql").await?;
```

#### `introspection_to_sdl(response: &IntrospectionResponse) -> String`

Converts an introspection response to SDL format.

```rust
let sdl = introspection_to_sdl(&response);
```

### Constants

#### `INTROSPECTION_QUERY: &str`

The standard GraphQL introspection query string. Includes:
- All schema types (scalars, objects, interfaces, unions, enums, input objects)
- Field definitions with arguments and deprecation
- Directive definitions with locations and arguments
- Type references with up to 7 levels of nesting

```rust
use graphql_introspect::INTROSPECTION_QUERY;

println!("{}", INTROSPECTION_QUERY);
```

### Types

#### IntrospectionResponse

The top-level introspection response:

```rust
pub struct IntrospectionResponse {
    pub data: IntrospectionData,
}

pub struct IntrospectionData {
    pub schema: IntrospectionSchema,
}
```

#### IntrospectionSchema

The complete schema information:

```rust
pub struct IntrospectionSchema {
    pub query_type: Option<IntrospectionTypeRef>,
    pub mutation_type: Option<IntrospectionTypeRef>,
    pub subscription_type: Option<IntrospectionTypeRef>,
    pub types: Vec<IntrospectionType>,
    pub directives: Vec<IntrospectionDirective>,
}
```

#### IntrospectionType

An enum representing all possible GraphQL type kinds:

```rust
pub enum IntrospectionType {
    Scalar(IntrospectionScalarType),
    Object(IntrospectionObjectType),
    Interface(IntrospectionInterfaceType),
    Union(IntrospectionUnionType),
    Enum(IntrospectionEnumType),
    InputObject(IntrospectionInputObjectType),
}
```

#### IntrospectionError

Error types for introspection operations:

```rust
pub enum IntrospectionError {
    Network(String),        // Connection failures, timeouts
    Http(u16, String),      // HTTP errors (non-2xx status)
    Parse(String),          // Invalid JSON
    Invalid(String),        // Malformed introspection data
}
```

## SDL Generation

The SDL generator produces clean, readable schema definitions:

### What Gets Filtered

**Built-in scalars:**
- `Int`, `Float`, `String`, `Boolean`, `ID`

**Introspection types:**
- Types starting with `__` (like `__Schema`, `__Type`, `__Field`)

**Built-in directives:**
- `@skip`, `@include`, `@deprecated`, `@specifiedBy`

### What Gets Preserved

- Custom scalar types
- Object types with fields
- Interface types
- Union types
- Enum types with values
- Input object types
- Custom directives
- Descriptions (as documentation strings)
- Deprecation information with reasons
- Default values for arguments and input fields

### Schema Definition Block

Only generated when necessary:
- Query type is not named "Query"
- Mutation type is not named "Mutation"
- Subscription type is not named "Subscription"

Otherwise, GraphQL's default root type names are used.

## Examples

### Save Schema to File

```rust
use graphql_introspect::introspect_url_to_sdl;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdl = introspect_url_to_sdl("https://api.example.com/graphql").await?;
    fs::write("schema.graphql", sdl)?;
    Ok(())
}
```

### Compare Two Schema Versions

```rust
use graphql_introspect::introspect_url_to_sdl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let production = introspect_url_to_sdl("https://api.example.com/graphql").await?;
    let staging = introspect_url_to_sdl("https://staging.example.com/graphql").await?;

    if production != staging {
        println!("Schemas differ!");
    }

    Ok(())
}
```

### Extract Custom Scalars

```rust
use graphql_introspect::{execute_introspection, IntrospectionType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let introspection = execute_introspection("https://api.example.com/graphql").await?;

    let built_in = ["Int", "Float", "String", "Boolean", "ID"];

    for type_def in &introspection.data.schema.types {
        if let IntrospectionType::Scalar(scalar) = type_def {
            if !built_in.contains(&scalar.name.as_str()) {
                println!("Custom scalar: {}", scalar.name);
            }
        }
    }

    Ok(())
}
```

### Error Handling

```rust
use graphql_introspect::{introspect_url_to_sdl, IntrospectionError};

#[tokio::main]
async fn main() {
    match introspect_url_to_sdl("https://api.example.com/graphql").await {
        Ok(sdl) => println!("{}", sdl),
        Err(IntrospectionError::Network(msg)) => {
            eprintln!("Network error: {}", msg);
        }
        Err(IntrospectionError::Http(status, body)) => {
            eprintln!("HTTP {} error: {}", status, body);
        }
        Err(IntrospectionError::Parse(msg)) => {
            eprintln!("Failed to parse response: {}", msg);
        }
        Err(IntrospectionError::Invalid(msg)) => {
            eprintln!("Invalid response: {}", msg);
        }
    }
}
```

## Implementation Details

### HTTP Client

Built on [reqwest](https://docs.rs/reqwest/) with:
- Automatic JSON serialization/deserialization
- Standard GraphQL POST request format (`{"query": "...", "variables": {}}`)
- Error handling for network and HTTP failures

### Type Conversion

The SDL conversion handles:
- Type reference unwrapping (NonNull and List wrappers)
- Description formatting (single-line vs multi-line)
- String escaping (quotes, newlines, backslashes)
- Proper GraphQL syntax for all type kinds
- Indentation for readability

### Introspection Query Depth

The introspection query supports up to 7 levels of type nesting:
```graphql
type {
  ofType {  # Level 1
    ofType {  # Level 2
      ofType {  # Level 3
        ofType {  # Level 4
          ofType {  # Level 5
            ofType {  # Level 6
              ofType {  # Level 7
                name
                kind
              }
            }
          }
        }
      }
    }
  }
}
```

This is sufficient for most GraphQL schemas.

## Inspiration

This crate is inspired by [introspector-gadget](https://docs.rs/introspector-gadget/).

## Dependencies

- `serde` and `serde_json` - JSON serialization/deserialization
- `reqwest` - HTTP client for introspection requests
- `tokio` - Async runtime
- `thiserror` - Error type definitions

## License

MIT OR Apache-2.0
