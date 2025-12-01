# graphql-introspect

GraphQL introspection query execution and SDL conversion.

## Purpose

This crate handles fetching GraphQL schemas from remote endpoints via introspection and converting them to Schema Definition Language (SDL). It provides:
- Standard GraphQL introspection query execution
- Type-safe deserialization of introspection responses
- Conversion from introspection JSON to SDL strings

## How it Fits

This is a utility crate used by graphql-project for loading remote schemas:

```
graphql-project -> graphql-introspect -> HTTP endpoint
```

When a GraphQL configuration specifies a URL as a schema source (e.g., `https://api.example.com/graphql`), this crate handles the introspection and conversion to SDL.

## Usage

### One-Step Introspection to SDL

The simplest way to use this crate:

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

For more control, you can execute introspection and convert to SDL separately:

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

### Working with Introspection Types

You can also work with the raw introspection data structures:

```rust
use graphql_introspect::{execute_introspection, IntrospectionResponse};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let introspection = execute_introspection("https://api.example.com/graphql").await?;

    // Access schema information
    let schema = &introspection.data.schema;

    for type_def in &schema.types {
        println!("Found type: {:?}", type_def);
    }

    Ok(())
}
```

## Features

### Introspection Query

The crate provides a standard GraphQL introspection query that includes:
- All schema types (scalars, objects, interfaces, unions, enums, input objects)
- Field definitions with arguments and deprecation information
- Directive definitions with locations and arguments
- Type references with full nesting (up to 7 levels of wrapping)

Access the query string directly:

```rust
use graphql_introspect::INTROSPECTION_QUERY;

println!("{}", INTROSPECTION_QUERY);
```

### SDL Generation

The SDL generator produces clean, readable schema definitions that:
- Skip built-in scalar types (Int, Float, String, Boolean, ID)
- Skip introspection types (types starting with `__`)
- Skip built-in directives (@skip, @include, @deprecated, @specifiedBy)
- Preserve descriptions as documentation strings
- Include deprecation directives with reasons
- Format types with proper indentation and syntax

### Type Safety

All introspection response fields are strongly typed:

```rust
use graphql_introspect::{IntrospectionType, IntrospectionObjectType};

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
```

## Error Handling

The crate provides clear error types for different failure modes:

```rust
use graphql_introspect::{introspect_url_to_sdl, IntrospectionError};

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
```

## Technical Details

### HTTP Client

Uses `reqwest` for HTTP requests with:
- Automatic JSON serialization/deserialization
- Standard GraphQL POST request format
- Error handling for network and HTTP failures

### Type Conversion

The SDL conversion handles:
- Type reference unwrapping (NonNull and List wrappers)
- Description formatting (single-line vs multi-line)
- String escaping (quotes, newlines, backslashes)
- Proper GraphQL syntax for all type kinds

### Schema Definition Generation

The crate generates a schema definition block only when necessary:
- If query type is not "Query"
- If mutation type is not "Mutation"
- If subscription type is not "Subscription"

Otherwise, it relies on GraphQL's default root type names.

## Inspiration

This crate is inspired by [introspector-gadget](https://docs.rs/introspector-gadget/) but tailored specifically for the graphql-lsp project's needs.

## Dependencies

- `serde` and `serde_json` - JSON serialization/deserialization
- `reqwest` - HTTP client for introspection requests
- `thiserror` - Error type definitions

## Development

Key files:
- [src/types.rs](src/types.rs) - Introspection response type definitions
- [src/query.rs](src/query.rs) - Introspection query execution
- [src/sdl.rs](src/sdl.rs) - SDL generation from introspection data
- [src/error.rs](src/error.rs) - Error types

When adding features:
1. Update types in types.rs if introspection query changes
2. Update SDL generation in sdl.rs for new type handling
3. Add tests to verify correctness
4. Update this README with usage examples
