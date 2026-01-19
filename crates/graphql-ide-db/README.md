# graphql-ide-db

Salsa database for GraphQL IDE features.

This crate provides `RootDatabase`, the central database type that implements all GraphQL-related Salsa traits. It serves as the high-level database that brings together the entire query-based architecture.

## Architecture

This crate follows the rust-analyzer pattern where the full database type is defined in a high-level crate rather than the foundation crate. This allows:

- Lower-level crates (`graphql-syntax`, `graphql-hir`, `graphql-analysis`) to define traits without circular dependencies
- Test utilities to depend on this crate for a complete database implementation
- Clean separation between trait definitions and implementations

## Usage

```rust
use graphql_ide_db::RootDatabase;

let db = RootDatabase::default();
// Use with any GraphQL query trait
```

## Trait Implementations

`RootDatabase` implements:

- `salsa::Database` - Core Salsa functionality
- `GraphQLSyntaxDatabase` - Parsing and syntax queries
- `GraphQLHirDatabase` - High-level IR queries
- `GraphQLAnalysisDatabase` - Validation and analysis queries
