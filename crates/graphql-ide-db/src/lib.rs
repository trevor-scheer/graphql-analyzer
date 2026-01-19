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
    DocumentFileIds, FileContent, FileEntry, FileEntryMap, FileId, FileKind, FileMetadata, FileUri,
    ProjectFiles, SchemaFileIds,
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

/// Query tracking for testing Salsa caching behavior.
///
/// This module provides utilities for verifying that Salsa's incremental computation
/// is working correctly. It uses Salsa's event callback mechanism to track when queries
/// are actually executed vs served from cache.
#[cfg(any(test, feature = "test-utils"))]
pub mod tracking {
    use salsa::{Event, EventKind, Storage};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Well-known query names to prevent typos in test assertions.
    pub mod queries {
        pub const PARSE: &str = "parse";
        pub const FILE_STRUCTURE: &str = "file_structure";
        pub const FILE_TYPE_DEFS: &str = "file_type_defs";
        pub const FILE_FRAGMENTS: &str = "file_fragments";
        pub const FILE_OPERATIONS: &str = "file_operations";
        pub const SCHEMA_TYPES: &str = "schema_types";
        pub const ALL_FRAGMENTS: &str = "all_fragments";
        pub const ALL_OPERATIONS: &str = "all_operations";
        pub const FILE_LOOKUP: &str = "file_lookup";
    }

    #[derive(Default)]
    struct QueryLog {
        executions: Vec<String>,
        counts: HashMap<String, usize>,
    }

    impl QueryLog {
        fn record(&mut self, query_name: &str) {
            self.executions.push(query_name.to_string());
            *self.counts.entry(query_name.to_string()).or_insert(0) += 1;
        }

        fn checkpoint(&self) -> usize {
            self.executions.len()
        }

        fn count_since(&self, query_name: &str, checkpoint: usize) -> usize {
            self.executions[checkpoint..]
                .iter()
                .filter(|n| n.as_str() == query_name)
                .count()
        }

        fn executions_since(&self, checkpoint: usize) -> Vec<String> {
            self.executions[checkpoint..].to_vec()
        }

        fn total_count(&self, query_name: &str) -> usize {
            self.counts.get(query_name).copied().unwrap_or(0)
        }

        fn all_counts(&self) -> HashMap<String, usize> {
            self.counts.clone()
        }

        fn reset(&mut self) {
            self.executions.clear();
            self.counts.clear();
        }
    }

    fn extract_query_name(database_key: &dyn std::fmt::Debug) -> String {
        let debug_str = format!("{database_key:?}");
        let without_args = debug_str.split('(').next().unwrap_or(&debug_str);
        without_args
            .rsplit("::")
            .next()
            .unwrap_or(without_args)
            .to_string()
    }

    /// A Salsa database that tracks query executions for testing.
    #[derive(Clone)]
    pub struct TrackedDatabase {
        storage: Storage<Self>,
        log: Arc<Mutex<QueryLog>>,
    }

    impl Default for TrackedDatabase {
        fn default() -> Self {
            Self::new()
        }
    }

    impl TrackedDatabase {
        #[must_use]
        pub fn new() -> Self {
            let log = Arc::new(Mutex::new(QueryLog::default()));
            let log_for_callback = Arc::clone(&log);

            Self {
                storage: Storage::new(Some(Box::new(move |event: Event| {
                    if let EventKind::WillExecute { database_key } = event.kind {
                        let query_name = extract_query_name(&database_key);
                        log_for_callback
                            .lock()
                            .expect("QueryLog mutex poisoned")
                            .record(&query_name);
                    }
                }))),
                log,
            }
        }

        fn with_log<F, R>(&self, f: F) -> R
        where
            F: FnOnce(&QueryLog) -> R,
        {
            f(&self.log.lock().expect("QueryLog mutex poisoned"))
        }

        #[must_use]
        pub fn checkpoint(&self) -> usize {
            self.with_log(QueryLog::checkpoint)
        }

        #[must_use]
        pub fn count_since(&self, query_name: &str, checkpoint: usize) -> usize {
            self.with_log(|log| log.count_since(query_name, checkpoint))
        }

        #[must_use]
        pub fn executions_since(&self, checkpoint: usize) -> Vec<String> {
            self.with_log(|log| log.executions_since(checkpoint))
        }

        #[must_use]
        pub fn total_count(&self, query_name: &str) -> usize {
            self.with_log(|log| log.total_count(query_name))
        }

        #[must_use]
        pub fn all_counts(&self) -> HashMap<String, usize> {
            self.with_log(QueryLog::all_counts)
        }

        pub fn reset(&self) {
            self.log.lock().expect("QueryLog mutex poisoned").reset();
        }
    }

    #[salsa::db]
    impl salsa::Database for TrackedDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TrackedDatabase {}

    #[salsa::db]
    impl graphql_hir::GraphQLHirDatabase for TrackedDatabase {}

    #[salsa::db]
    impl graphql_analysis::GraphQLAnalysisDatabase for TrackedDatabase {}

    // SAFETY: storage/storage_mut return references to the owned storage field
    unsafe impl salsa::plumbing::HasStorage for TrackedDatabase {
        fn storage(&self) -> &Storage<Self> {
            &self.storage
        }

        fn storage_mut(&mut self) -> &mut Storage<Self> {
            &mut self.storage
        }
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
            FileKind::Schema,
        );

        let parse = graphql_syntax::parse(&db, content, metadata);
        assert!(parse.errors().is_empty());
    }
}
