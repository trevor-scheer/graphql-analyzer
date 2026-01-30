//! Shared test database implementations.
//!
//! This module provides access to test databases for GraphQL LSP testing.
//!
//! ## Available Databases
//!
//! - `RootDatabase` - The standard Salsa database with all query traits implemented.
//!   Re-exported from `graphql_ide_db`.
//!
//! - `TestDatabaseWithProject` - A database variant that stores `ProjectFiles` in a Cell,
//!   allowing tests to set project context after database construction.

use graphql_base_db::{
    DocumentFileIds, FileContent, FileEntry, FileEntryMap, FileId, FileKind, FileMetadata, FileUri,
    ProjectFiles, SchemaFileIds,
};
use std::collections::HashMap;
use std::sync::Arc;

// Re-export RootDatabase from graphql-ide-db for convenience
pub use graphql_ide_db::RootDatabase;

/// Backward-compatible alias for RootDatabase.
pub type TestDatabase = RootDatabase;

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

/// Helper to create `ProjectFiles` for tests.
///
/// This function takes lists of schema and document files and creates
/// the proper granular Salsa inputs (`SchemaFileIds`, `DocumentFileIds`, `FileEntryMap`).
pub fn create_project_files<DB: salsa::Database>(
    db: &mut DB,
    schema_files: &[(FileId, FileContent, FileMetadata)],
    document_files: &[(FileId, FileContent, FileMetadata)],
) -> ProjectFiles {
    let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
    let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

    let mut entries = HashMap::new();
    for (id, content, metadata) in schema_files {
        let entry = FileEntry::new(db, *content, *metadata);
        entries.insert(*id, entry);
    }
    for (id, content, metadata) in document_files {
        let entry = FileEntry::new(db, *content, *metadata);
        entries.insert(*id, entry);
    }

    let schema_file_ids = SchemaFileIds::new(db, Arc::new(schema_ids));
    let document_file_ids = DocumentFileIds::new(db, Arc::new(doc_ids));
    let file_entry_map = FileEntryMap::new(db, Arc::new(entries));

    ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
}
