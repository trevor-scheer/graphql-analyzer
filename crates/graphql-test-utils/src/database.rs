//! Shared test database implementations.
//!
//! This module provides pre-configured test databases that implement all the
//! necessary Salsa traits for GraphQL LSP testing. Use these instead of defining
//! your own `TestDatabase` in each test module.
//!
//! ## Feature Flags
//!
//! - `analysis`: Adds `GraphQLAnalysisDatabase` impl to `TestDatabase`.
//!   Only enable for crates that don't transitively depend on graphql-analysis.

use std::sync::Arc;

use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};

/// Test database with GraphQL LSP traits implemented.
///
/// By default, implements syntax and HIR traits. Enable the `analysis` feature
/// to also implement `GraphQLAnalysisDatabase`.
///
/// # Example
///
/// ```ignore
/// use graphql_test_utils::TestDatabase;
/// use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri};
///
/// let db = TestDatabase::default();
/// let content = FileContent::new(&db, "type Query { hello: String }".into());
/// let metadata = FileMetadata::new(
///     &db,
///     FileId::new(0),
///     FileUri::new("schema.graphql"),
///     FileKind::Schema,
/// );
/// ```
#[salsa::db]
#[derive(Clone, Default)]
pub struct TestDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for TestDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for TestDatabase {}

#[cfg(feature = "analysis")]
#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for TestDatabase {}

/// Test database with project files support via Cell.
///
/// Some tests need to set `project_files()` before calling validation functions
/// that look up project context. This database variant stores project files
/// in a `Cell` that can be set after construction.
///
/// # Example
///
/// ```ignore
/// use graphql_test_utils::TestDatabaseWithProject;
///
/// let db = TestDatabaseWithProject::default();
/// let project_files = create_project_files(&db, &schemas, &docs);
/// db.set_project_files(Some(project_files));
///
/// // Now validation can access project context
/// let diagnostics = validate_document_file(&db, content, metadata);
/// ```
#[salsa::db]
#[derive(Clone)]
pub struct TestDatabaseWithProject {
    storage: salsa::Storage<Self>,
    project_files: std::cell::Cell<Option<ProjectFiles>>,
}

impl Default for TestDatabaseWithProject {
    fn default() -> Self {
        Self {
            storage: salsa::Storage::default(),
            project_files: std::cell::Cell::new(None),
        }
    }
}

impl TestDatabaseWithProject {
    /// Set the project files for this database.
    pub fn set_project_files(&self, project_files: Option<ProjectFiles>) {
        self.project_files.set(project_files);
    }
}

#[salsa::db]
impl salsa::Database for TestDatabaseWithProject {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabaseWithProject {}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for TestDatabaseWithProject {
    fn project_files(&self) -> Option<ProjectFiles> {
        self.project_files.get()
    }
}

#[cfg(feature = "analysis")]
#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for TestDatabaseWithProject {}

/// Create a file content Salsa input from a string.
pub fn file_content(db: &impl salsa::Database, content: &str) -> FileContent {
    FileContent::new(db, Arc::from(content))
}

/// Create file metadata Salsa input.
pub fn file_metadata(
    db: &impl salsa::Database,
    id: u32,
    uri: &str,
    kind: FileKind,
) -> FileMetadata {
    FileMetadata::new(db, FileId::new(id), FileUri::new(uri), kind)
}
