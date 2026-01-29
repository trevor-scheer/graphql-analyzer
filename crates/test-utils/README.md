# graphql-test-utils

Shared test utilities for the GraphQL LSP crates.

## Overview

This crate provides consistent patterns and utilities for testing across the entire GraphQL LSP codebase. Instead of each crate defining its own `TestDatabase` and helper functions, they can import from this shared crate.

## Features

- **TestDatabase**: Pre-configured Salsa databases with all GraphQL LSP traits implemented
- **Project builders**: Simple and fluent APIs for creating test projects
- **Cursor extraction**: Helpers for IDE feature tests that need cursor positions
- **Shared fixtures**: Common schema and document fixtures for testing

## Usage

```rust
use graphql_test_utils::{test_project, fixtures::BASIC_SCHEMA};

#[test]
fn test_valid_query() {
    let (db, project) = test_project(
        BASIC_SCHEMA,
        "query { user(id: \"1\") { id name } }",
    );
    // ... run validation and assertions
}
```

For more complex scenarios:

```rust
use graphql_test_utils::TestProjectBuilder;

let (db, project) = TestProjectBuilder::new()
    .with_schema("schema.graphql", SCHEMA)
    .with_document("fragments.graphql", FRAGMENTS)
    .with_document("queries.graphql", QUERIES)
    .build();
```

## Modules

- `database` - Test database implementations
- `project` - Project creation helpers and builders
- `cursor` - Cursor position extraction for IDE tests
- `fixtures` - Shared schema and document fixtures
