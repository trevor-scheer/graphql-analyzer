// GraphQL Database Layer
// This crate defines the salsa database and input queries for the GraphQL LSP.
// It provides the foundation for incremental, query-based computation.

use std::sync::Arc;

/// Input file identifier in the project
/// We use a simple u32-based ID for now
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(u32);

impl FileId {
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// A URI string (file:// or relative path)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileUri(Arc<str>);

impl FileUri {
    #[must_use]
    pub fn new(uri: impl Into<Arc<str>>) -> Self {
        Self(uri.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FileUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// File kind discriminator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// GraphQL schema file (.graphql, .gql with type definitions)
    Schema,
    /// Pure executable GraphQL file (.graphql, .gql with operations/fragments)
    ExecutableGraphQL,
    /// TypeScript file with embedded GraphQL
    TypeScript,
    /// JavaScript file with embedded GraphQL
    JavaScript,
}

/// Input: Content of a file
/// This is set by the LSP layer when files are opened/changed
#[salsa::input]
pub struct FileContent {
    pub text: Arc<str>,
}

/// Input: Metadata about a file
/// This is set by the LSP layer when files are added to the project
#[salsa::input]
pub struct FileMetadata {
    pub file_id: FileId,
    pub uri: FileUri,
    pub kind: FileKind,
}

/// Input: Project file lists
/// Tracks which files are in the project, categorized by kind
/// This is updated by the IDE layer when files are added/removed
#[salsa::input]
pub struct ProjectFiles {
    /// List of schema files with their content and metadata
    pub schema_files: Arc<Vec<(FileId, FileContent, FileMetadata)>>,
    /// List of document files with their content and metadata
    pub document_files: Arc<Vec<(FileId, FileContent, FileMetadata)>>,
}

/// The root salsa database
/// This is the main entry point for all queries
#[salsa::db]
#[derive(Clone, Default)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

// Implement the Database trait
#[salsa::db]
impl salsa::Database for RootDatabase {}

impl RootDatabase {
    /// Create a new database
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// A batch of changes to apply to the database atomically
#[derive(Debug, Default)]
pub struct Change {
    /// Files whose content has changed (`file_id`, `new_content`)
    pub files_changed: Vec<(FileId, Arc<str>)>,
    /// Files that have been removed from the project
    pub files_removed: Vec<FileId>,
    /// Files that have been added to the project (uri, content, kind)
    pub files_added: Vec<(FileUri, Arc<str>, FileKind)>,
}

impl RootDatabase {
    /// Apply a batch of changes to the database
    /// This will automatically invalidate dependent queries via salsa
    ///
    /// Note: This is a simplified implementation for Phase 1.
    /// A complete implementation will include a `FileRegistry` to map URIs to `FileIds`.
    pub fn apply_change(&mut self, _change: Change) {
        // Placeholder implementation for Phase 1
        // Full implementation will come when we add FileRegistry
        // For now, we just accept changes but don't process them
        // This is sufficient for initial testing of the database structure
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use salsa::Setter;

    #[test]
    fn test_database_creation() {
        let _db = RootDatabase::new();
        // Database should be created successfully
    }

    #[test]
    fn test_file_id() {
        let file_id = FileId::new(42);
        assert_eq!(file_id.as_u32(), 42);
    }

    #[test]
    fn test_file_uri() {
        let uri = FileUri::new("file:///path/to/file.graphql");
        assert_eq!(uri.as_str(), "file:///path/to/file.graphql");
        assert_eq!(uri.to_string(), "file:///path/to/file.graphql");
    }

    #[test]
    fn test_file_kind() {
        let kinds = [
            FileKind::Schema,
            FileKind::ExecutableGraphQL,
            FileKind::TypeScript,
            FileKind::JavaScript,
        ];

        // All kinds should be distinct
        for (i, kind1) in kinds.iter().enumerate() {
            for (j, kind2) in kinds.iter().enumerate() {
                if i == j {
                    assert_eq!(kind1, kind2);
                } else {
                    assert_ne!(kind1, kind2);
                }
            }
        }
    }

    #[test]
    fn test_file_content_creation() {
        let db = RootDatabase::new();
        let content: Arc<str> = Arc::from("type Query { hello: String }");
        let file_content = FileContent::new(&db, content);

        // File content should be stored and retrievable
        assert_eq!(
            file_content.text(&db).as_ref(),
            "type Query { hello: String }"
        );
    }

    #[test]
    fn test_file_metadata_creation() {
        let db = RootDatabase::new();
        let file_id = FileId::new(0);
        let uri = FileUri::new("file:///test.graphql");
        let kind = FileKind::Schema;

        let metadata = FileMetadata::new(&db, file_id, uri.clone(), kind);

        // Metadata should be stored and retrievable
        assert_eq!(metadata.file_id(&db), file_id);
        assert_eq!(metadata.uri(&db), uri);
        assert_eq!(metadata.kind(&db), kind);
    }

    #[test]
    fn test_file_content_update() {
        let mut db = RootDatabase::new();
        let content1: Arc<str> = Arc::from("type Query { hello: String }");
        let file_content = FileContent::new(&db, content1);

        // Initial content
        assert_eq!(
            file_content.text(&db).as_ref(),
            "type Query { hello: String }"
        );

        // Update content
        let content2: Arc<str> = Arc::from("type Query { world: String }");
        file_content.set_text(&mut db).to(content2);

        // Content should be updated
        assert_eq!(
            file_content.text(&db).as_ref(),
            "type Query { world: String }"
        );
    }

    #[test]
    fn test_change_application() {
        let mut db = RootDatabase::new();

        let change = Change {
            files_added: vec![(
                FileUri::new("file:///test.graphql"),
                Arc::from("type Query { hello: String }"),
                FileKind::Schema,
            )],
            ..Default::default()
        };

        db.apply_change(change);

        // Change should be applied successfully
        // Detailed verification will come with FileRegistry implementation
    }
}
