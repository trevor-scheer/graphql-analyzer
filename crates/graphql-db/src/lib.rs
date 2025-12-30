// GraphQL Database Layer
// This crate defines the salsa database and input queries for the GraphQL LSP.
// It provides the foundation for incremental, query-based computation.

use std::collections::HashMap;
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

/// Input: Schema file ID list (identity only)
/// This input changes ONLY when schema files are added or removed.
/// Content changes do NOT affect this input, enabling fine-grained cache invalidation.
#[salsa::input]
pub struct SchemaFileIds {
    /// List of schema file IDs - stable across content changes
    pub ids: Arc<Vec<FileId>>,
}

/// Input: Document file ID list (identity only)
/// This input changes ONLY when document files are added or removed.
/// Content changes do NOT affect this input, enabling fine-grained cache invalidation.
#[salsa::input]
pub struct DocumentFileIds {
    /// List of document file IDs - stable across content changes
    pub ids: Arc<Vec<FileId>>,
}

/// Input: File lookup map
/// Maps FileId to (FileContent, FileMetadata) for O(1) lookup.
/// This input changes when any file's content changes, but queries that only
/// need the file list (not content) should depend on SchemaFileIds/DocumentFileIds instead.
#[salsa::input]
pub struct FileMap {
    /// Mapping from FileId to file content and metadata
    pub entries: Arc<HashMap<FileId, (FileContent, FileMetadata)>>,
}

/// Input: Project file tracking with granular inputs
/// This struct provides access to both file identity (stable) and file content (dynamic).
///
/// Queries should choose their dependencies carefully:
/// - Depend on `schema_file_ids` or `document_file_ids` for "what files exist" (stable)
/// - Depend on `file_map` for "what's in the files" (changes on content edit)
/// - Call per-file queries with specific FileContent to get per-file caching
#[salsa::input]
pub struct ProjectFiles {
    /// Schema file IDs - only changes when schema files are added/removed
    pub schema_file_ids: SchemaFileIds,
    /// Document file IDs - only changes when document files are added/removed
    pub document_file_ids: DocumentFileIds,
    /// File content/metadata map - changes when any file content changes
    pub file_map: FileMap,
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

/// Test utilities for creating project files
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    use super::*;

    /// Helper to create ProjectFiles for tests
    ///
    /// This function takes lists of schema and document files and creates
    /// the proper granular Salsa inputs (SchemaFileIds, DocumentFileIds, FileMap).
    pub fn create_project_files<DB: salsa::Database>(
        db: &DB,
        schema_files: Vec<(FileId, FileContent, FileMetadata)>,
        document_files: Vec<(FileId, FileContent, FileMetadata)>,
    ) -> ProjectFiles {
        let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
        let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

        let mut entries = HashMap::new();
        for (id, content, metadata) in &schema_files {
            entries.insert(*id, (*content, *metadata));
        }
        for (id, content, metadata) in &document_files {
            entries.insert(*id, (*content, *metadata));
        }

        let schema_file_ids = SchemaFileIds::new(db, Arc::new(schema_ids));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(doc_ids));
        let file_map = FileMap::new(db, Arc::new(entries));

        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_map)
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
