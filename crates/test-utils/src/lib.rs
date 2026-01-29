//! # GraphQL Test Utilities
//!
//! Shared test infrastructure for the GraphQL LSP crates. This crate provides
//! consistent patterns and utilities for testing across the entire codebase.

// Test utilities are less strict than production code
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::doc_markdown)]
//!
//! ## Quick Start
//!
//! For most tests, use the simple helpers:
//!
//! ```ignore
//! use graphql_test_utils::{test_project, fixtures::BASIC_SCHEMA};
//!
//! #[test]
//! fn test_valid_query() {
//!     let (db, project) = test_project(
//!         BASIC_SCHEMA,
//!         "query { user(id: \"1\") { id name } }",
//!     );
//!     // ... assertions
//! }
//! ```
//!
//! ## Modules
//!
//! - [`database`] - Pre-configured test databases with Salsa traits
//! - [`project`] - Helpers and builders for creating test projects
//! - [`cursor`] - Cursor position extraction for IDE feature tests
//! - [`fixtures`] - Common schema and document fixtures
//!
//! ## Re-exports
//!
//! The most commonly used items are re-exported at the crate root for convenience.

pub mod assertions;
pub mod cursor;
pub mod database;
pub mod fixtures;
pub mod project;
pub mod tracking;

// Re-export commonly used items at crate root
pub use cursor::{extract_cursor, extract_cursors, Position};
pub use database::{
    create_project_files, file_content, file_metadata, RootDatabase, TestDatabase,
    TestDatabaseWithProject,
};
pub use project::{test_documents_only, test_project, test_schema_only, TestProjectBuilder};
pub use tracking::{queries, TrackedDatabase};

// Re-export common types needed for test setup
pub use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};

// Re-export insta for snapshot testing
pub use insta;

// Re-export assertion helpers
pub use assertions::{format_diagnostic_messages, format_diagnostics};
