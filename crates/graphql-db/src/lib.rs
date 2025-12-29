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
    /// Line offset for extracted GraphQL (0 for pure GraphQL files)
    /// For TypeScript/JavaScript files, this is the line number where the GraphQL starts
    #[default]
    pub line_offset: u32,
}

/// Input: Schema file list
/// Tracks which schema files are in the project
/// This is a separate input from DocumentFiles to enable fine-grained invalidation:
/// changing document files won't invalidate queries that only depend on schema files
#[salsa::input]
pub struct SchemaFiles {
    /// List of schema files with their content and metadata
    pub files: Arc<Vec<(FileId, FileContent, FileMetadata)>>,
}

/// Input: Document file list
/// Tracks which document files (operations/fragments) are in the project
/// This is a separate input from SchemaFiles to enable fine-grained invalidation:
/// changing schema files won't invalidate queries that only depend on document files
#[salsa::input]
pub struct DocumentFiles {
    /// List of document files with their content and metadata
    pub files: Arc<Vec<(FileId, FileContent, FileMetadata)>>,
}

/// Input: Project file lists
/// Tracks which files are in the project, categorized by kind
/// This is updated by the IDE layer when files are added/removed
///
/// Note: This struct now just holds references to the separate SchemaFiles and DocumentFiles
/// inputs. Queries should prefer using the specific input they need to minimize invalidation scope.
#[salsa::input]
pub struct ProjectFiles {
    /// Schema files input
    pub schema_files: SchemaFiles,
    /// Document files input
    pub document_files: DocumentFiles,
}

/// The root salsa database
/// This is the main entry point for all queries
#[salsa::db]
#[derive(Clone, Default)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

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
    use salsa::Setter;

    #[test]
    fn test_database_creation() {
        let _db = RootDatabase::new();
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

        assert_eq!(metadata.file_id(&db), file_id);
        assert_eq!(metadata.uri(&db), uri);
        assert_eq!(metadata.kind(&db), kind);
    }

    #[test]
    fn test_file_content_update() {
        let mut db = RootDatabase::new();
        let content1: Arc<str> = Arc::from("type Query { hello: String }");
        let file_content = FileContent::new(&db, content1);

        assert_eq!(
            file_content.text(&db).as_ref(),
            "type Query { hello: String }"
        );

        let content2: Arc<str> = Arc::from("type Query { world: String }");
        file_content.set_text(&mut db).to(content2);

        assert_eq!(
            file_content.text(&db).as_ref(),
            "type Query { world: String }"
        );
    }
}
