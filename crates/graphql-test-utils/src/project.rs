//! Test project builders and helpers.
//!
//! This module provides convenient ways to create test projects with schemas
//! and documents. Use the simple functions for common cases and the builder
//! for complex multi-file scenarios.

use std::sync::Arc;

use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};

use crate::TestDatabase;

/// Simple helper to create a test project with one schema and one document.
///
/// This is the most common test setup pattern. For more complex scenarios
/// with multiple files, use [`TestProjectBuilder`].
///
/// # Example
///
/// ```ignore
/// use graphql_test_utils::test_project;
///
/// let (db, project) = test_project(
///     "type Query { user: User } type User { id: ID! }",
///     "query { user { id } }",
/// );
///
/// // Now use db and project for validation
/// ```
pub fn test_project(schema: &str, document: &str) -> (TestDatabase, ProjectFiles) {
    TestProjectBuilder::new()
        .with_schema("schema.graphql", schema)
        .with_document("query.graphql", document)
        .build()
}

/// Simple helper to create a test project with only a schema (no documents).
///
/// Useful for testing schema validation in isolation.
pub fn test_schema_only(schema: &str) -> (TestDatabase, ProjectFiles) {
    TestProjectBuilder::new()
        .with_schema("schema.graphql", schema)
        .build()
}

/// Simple helper to create a test project with only documents (no schema).
///
/// Useful for testing document-only validation scenarios.
pub fn test_documents_only(documents: &[(&str, &str)]) -> (TestDatabase, ProjectFiles) {
    let mut builder = TestProjectBuilder::new();
    for (name, content) in documents {
        builder = builder.with_document(name, content);
    }
    builder.build()
}

/// Builder for complex test projects with multiple files.
///
/// Use this when you need:
/// - Multiple schema files
/// - Multiple document files
/// - Custom file names or URIs
/// - Fragment files separate from operation files
///
/// # Example
///
/// ```ignore
/// use graphql_test_utils::TestProjectBuilder;
///
/// let (db, project) = TestProjectBuilder::new()
///     .with_schema("schema.graphql", SCHEMA)
///     .with_document("fragments.graphql", "fragment UserFields on User { id name }")
///     .with_document("queries.graphql", "query { user { ...UserFields } }")
///     .build();
/// ```
#[derive(Default)]
pub struct TestProjectBuilder {
    schemas: Vec<(String, String)>,
    documents: Vec<(String, String)>,
}

impl TestProjectBuilder {
    /// Create a new empty project builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a schema file to the project.
    ///
    /// The name should be a filename like "schema.graphql". It will be
    /// converted to a file URI like "file:///schema.graphql".
    pub fn with_schema(mut self, name: &str, content: &str) -> Self {
        self.schemas.push((name.to_string(), content.to_string()));
        self
    }

    /// Add a document file to the project.
    ///
    /// The name should be a filename like "query.graphql". It will be
    /// converted to a file URI like "file:///query.graphql".
    pub fn with_document(mut self, name: &str, content: &str) -> Self {
        self.documents.push((name.to_string(), content.to_string()));
        self
    }

    /// Build the test database and project files.
    ///
    /// Returns a tuple of (database, project_files) ready for testing.
    pub fn build(self) -> (TestDatabase, ProjectFiles) {
        let mut db = TestDatabase::default();
        let mut file_id_counter = 0u32;

        let mut schema_tuples = Vec::new();
        for (name, content) in &self.schemas {
            let id = FileId::new(file_id_counter);
            file_id_counter += 1;

            let uri = format!("file:///{name}");
            let file_content = FileContent::new(&db, Arc::from(content.as_str()));
            let metadata = FileMetadata::new(&db, id, FileUri::new(uri), FileKind::Schema);

            schema_tuples.push((id, file_content, metadata));
        }

        let mut doc_tuples = Vec::new();
        for (name, content) in &self.documents {
            let id = FileId::new(file_id_counter);
            file_id_counter += 1;

            let uri = format!("file:///{name}");
            let file_content = FileContent::new(&db, Arc::from(content.as_str()));
            let metadata =
                FileMetadata::new(&db, id, FileUri::new(uri), FileKind::ExecutableGraphQL);

            doc_tuples.push((id, file_content, metadata));
        }

        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_tuples, &doc_tuples);

        (db, project_files)
    }
}

/// Result of building a test project, providing access to individual file components.
///
/// This is useful when you need to reference specific files for validation calls.
pub struct TestProject {
    /// The test database
    pub db: TestDatabase,
    /// The project files
    pub project_files: ProjectFiles,
    /// Schema files indexed by name
    pub schemas: Vec<TestFile>,
    /// Document files indexed by name
    pub documents: Vec<TestFile>,
}

/// A file in a test project with all its components.
#[derive(Clone)]
pub struct TestFile {
    /// The file ID
    pub id: FileId,
    /// The file content (Salsa input)
    pub content: FileContent,
    /// The file metadata (Salsa input)
    pub metadata: FileMetadata,
}

impl TestProjectBuilder {
    /// Build with detailed file access.
    ///
    /// Use this when you need to access individual file components for
    /// validation calls that require (content, metadata, project_files).
    pub fn build_detailed(self) -> TestProject {
        let mut db = TestDatabase::default();
        let mut file_id_counter = 0u32;

        let mut schema_files = Vec::new();
        let mut schema_tuples = Vec::new();
        for (name, content) in &self.schemas {
            let id = FileId::new(file_id_counter);
            file_id_counter += 1;

            let uri = format!("file:///{name}");
            let file_content = FileContent::new(&db, Arc::from(content.as_str()));
            let metadata = FileMetadata::new(&db, id, FileUri::new(uri), FileKind::Schema);

            schema_tuples.push((id, file_content, metadata));
            schema_files.push(TestFile {
                id,
                content: file_content,
                metadata,
            });
        }

        let mut doc_files = Vec::new();
        let mut doc_tuples = Vec::new();
        for (name, content) in &self.documents {
            let id = FileId::new(file_id_counter);
            file_id_counter += 1;

            let uri = format!("file:///{name}");
            let file_content = FileContent::new(&db, Arc::from(content.as_str()));
            let metadata =
                FileMetadata::new(&db, id, FileUri::new(uri), FileKind::ExecutableGraphQL);

            doc_tuples.push((id, file_content, metadata));
            doc_files.push(TestFile {
                id,
                content: file_content,
                metadata,
            });
        }

        let project_files =
            graphql_base_db::test_utils::create_project_files(&mut db, &schema_tuples, &doc_tuples);

        TestProject {
            db,
            project_files,
            schemas: schema_files,
            documents: doc_files,
        }
    }
}
