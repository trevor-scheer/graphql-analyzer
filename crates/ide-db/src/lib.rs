//! GraphQL IDE Database
//!
//! This crate provides the central Salsa database (`RootDatabase`) that implements
//! all query traits from the GraphQL LSP stack:
//!
//! - `salsa::Database` - Salsa's base trait
//! - `GraphQLSyntaxDatabase` - Parsing and line index queries
//! - `GraphQLHirDatabase` - HIR construction queries
//! - `GraphQLAnalysisDatabase` - Validation and analysis queries
//!
//! ## Architecture
//!
//! Following rust-analyzer's pattern, trait definitions live in their respective
//! crates (graphql-syntax, graphql-hir, graphql-analysis), but the implementation
//! of those traits for `RootDatabase` is centralized here. This allows:
//!
//! - Lower-level crates to define queries without knowing about the final database
//! - A single place to compose all database functionality
//! - Clean dependency ordering without cycles
//!
//! ## Usage
//!
//! ```ignore
//! use graphql_ide_db::RootDatabase;
//!
//! let db = RootDatabase::default();
//! // Now you can use queries from any level:
//! // - graphql_syntax::parse(&db, content, metadata)
//! // - graphql_hir::schema_types(&db, project_files)
//! // - graphql_analysis::file_diagnostics(&db, file_id, content, metadata, project_files)
//! ```

// Re-export commonly used types from the foundation crate
pub use graphql_base_db::{
    DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
    FileUri, Language, ProjectFiles, SchemaFileIds,
};

/// The root Salsa database for GraphQL IDE features.
///
/// This is the main entry point for all queries. It implements all database traits
/// from the query stack, allowing access to parsing, HIR, and analysis queries.
#[salsa::db]
#[derive(Clone, Default)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for RootDatabase {}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for RootDatabase {}

#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for RootDatabase {}

impl RootDatabase {
    /// Create a new database
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_database_creation() {
        let _db = RootDatabase::new();
    }

    #[test]
    fn test_database_can_parse() {
        let db = RootDatabase::new();
        let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata = FileMetadata::new(
            &db,
            FileId::new(0),
            FileUri::new("test.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let parse = graphql_syntax::parse(&db, content, metadata);
        assert!(parse.errors().is_empty());
    }
}
