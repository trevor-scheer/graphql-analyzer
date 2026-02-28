//! Query tracking for testing Salsa caching behavior.
//!
//! This module provides utilities for verifying that Salsa's incremental computation
//! is working correctly. It uses Salsa's event callback mechanism to track when queries
//! are actually executed vs served from cache.
//!
//! ## Usage
//!
//! ```ignore
//! use graphql_test_utils::tracking::{TrackedDatabase, queries};
//!
//! let mut db = TrackedDatabase::new();
//!
//! // Cold query - should execute
//! let checkpoint = db.checkpoint();
//! let types = schema_types(&db, project_files);
//! assert!(db.count_since(queries::SCHEMA_TYPES, checkpoint) >= 1);
//!
//! // Warm query - should be cached
//! let checkpoint2 = db.checkpoint();
//! let types2 = schema_types(&db, project_files);
//! assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint2), 0);
//! ```

use salsa::{Event, EventKind, Storage};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Well-known query names to prevent typos in test assertions.
///
/// These match the function names of Salsa tracked queries.
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
    pub const FILE_USED_FRAGMENT_NAMES: &str = "file_used_fragment_names";
    pub const FILE_DEFINED_FRAGMENT_NAMES: &str = "file_defined_fragment_names";
    pub const FILE_OPERATION_NAMES: &str = "file_operation_names";
    pub const FILE_SCHEMA_COORDINATES: &str = "file_schema_coordinates";
    pub const INTERFACE_IMPLEMENTORS: &str = "interface_implementors";
    pub const ALL_USED_SCHEMA_COORDINATES: &str = "all_used_schema_coordinates";
    pub const ALL_USED_FRAGMENT_NAMES: &str = "all_used_fragment_names";
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
///
/// Each instance has its own query log, making tests hermetic and
/// avoiding parallel test interference.
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

    /// Get the current checkpoint (log position) for later comparison.
    pub fn checkpoint(&self) -> usize {
        self.with_log(QueryLog::checkpoint)
    }

    /// Count executions of a specific query since the given checkpoint.
    pub fn count_since(&self, query_name: &str, checkpoint: usize) -> usize {
        self.with_log(|log| log.count_since(query_name, checkpoint))
    }

    /// Get all query executions since the given checkpoint.
    pub fn executions_since(&self, checkpoint: usize) -> Vec<String> {
        self.with_log(|log| log.executions_since(checkpoint))
    }

    /// Get total execution count for a query.
    pub fn total_count(&self, query_name: &str) -> usize {
        self.with_log(|log| log.total_count(query_name))
    }

    /// Get all query counts.
    pub fn all_counts(&self) -> HashMap<String, usize> {
        self.with_log(QueryLog::all_counts)
    }

    /// Reset all tracking data.
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
