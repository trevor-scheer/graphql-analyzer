use std::collections::HashMap;
use std::sync::Arc;

// Re-export types from graphql-types
pub use graphql_types::{DocumentKind, FileId, FileUri, Language};

/// Input: Content of a file
/// This is set by the LSP layer when files are opened/changed
#[salsa::input]
pub struct FileContent {
    pub text: Arc<str>,
}

/// Input: Metadata about a file
/// This is set by the LSP layer when files are added to the project
///
/// Files are classified along two orthogonal dimensions:
/// - `language`: How to parse the file (GraphQL, TypeScript, JavaScript, etc.)
/// - `document_kind`: What the content represents (Schema or Executable)
#[salsa::input]
pub struct FileMetadata {
    pub file_id: FileId,
    pub uri: FileUri,
    /// Source language - determines parsing strategy
    pub language: Language,
    /// Document kind - determines semantic processing
    pub document_kind: DocumentKind,
}

impl FileMetadata {
    /// Returns true if this is a schema file
    pub fn is_schema(&self, db: &dyn salsa::Database) -> bool {
        self.document_kind(db).is_schema()
    }

    /// Returns true if this is a document file (operations/fragments)
    pub fn is_document(&self, db: &dyn salsa::Database) -> bool {
        self.document_kind(db).is_executable()
    }

    /// Returns true if this file requires extraction (TS/JS files)
    pub fn requires_extraction(&self, db: &dyn salsa::Database) -> bool {
        self.language(db).requires_extraction()
    }
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

/// A single file's entry - bundles content and metadata as one Salsa input.
/// This enables true per-file granular caching: when file A changes, only
/// file A's FileEntry is updated, and queries for file B remain cached.
#[salsa::input]
pub struct FileEntry {
    /// The file's content
    pub content: FileContent,
    /// The file's metadata
    pub metadata: FileMetadata,
}

/// Input: Per-file entry map for granular invalidation
/// Unlike FileMap which stores all entries in a single HashMap (causing global invalidation),
/// this stores individual FileEntry inputs that can be updated independently.
///
/// When file A's content changes:
/// - FileEntryMap's HashMap reference stays the same (same keys)
/// - Only file A's FileEntry.content is updated
/// - Queries depending on file B's FileEntry remain fully cached
#[salsa::input]
pub struct FileEntryMap {
    /// Mapping from FileId to FileEntry - each entry is independently tracked
    pub entries: Arc<HashMap<FileId, FileEntry>>,
}

/// Input: Project file tracking with granular inputs
/// This struct provides access to both file identity (stable) and file content (dynamic).
///
/// Queries should choose their dependencies carefully:
/// - Depend on `schema_file_ids` or `document_file_ids` for "what files exist" (stable)
/// - Depend on `file_entry_map` for per-file granular lookup
/// - Call per-file queries with specific `FileContent` to get per-file caching
#[salsa::input]
pub struct ProjectFiles {
    /// Schema file IDs - only changes when schema files are added/removed
    pub schema_file_ids: SchemaFileIds,
    /// Document file IDs - only changes when document files are added/removed
    pub document_file_ids: DocumentFileIds,
    /// Per-file entry map for granular invalidation
    /// Each `FileEntry` can be updated independently without invalidating other files
    pub file_entry_map: FileEntryMap,
}

/// Query to look up a single file's content and metadata.
///
/// Uses `FileEntryMap` for granular per-file caching:
/// - Each file has its own `FileEntry` input
/// - Updating file A's content doesn't invalidate queries for file B
/// - The `HashMap` lookup creates a dependency only on the specific `FileEntry`
#[salsa::tracked]
pub fn file_lookup(
    db: &dyn salsa::Database,
    project_files: ProjectFiles,
    file_id: FileId,
) -> Option<(FileContent, FileMetadata)> {
    let file_entry_map = project_files.file_entry_map(db);
    let entries = file_entry_map.entries(db);
    let entry = entries.get(&file_id)?;
    Some((entry.content(db), entry.metadata(db)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use salsa::Setter;

    /// Simple test database for graphql-db tests.
    /// Only implements `salsa::Database` - no higher-level query traits.
    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[test]
    fn test_database_creation() {
        let _db = TestDatabase::default();
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
    fn test_language_and_document_kind() {
        // Test Language variants
        assert!(!Language::GraphQL.requires_extraction());
        assert!(Language::TypeScript.requires_extraction());
        assert!(Language::JavaScript.requires_extraction());

        // Test DocumentKind variants
        assert!(DocumentKind::Schema.is_schema());
        assert!(!DocumentKind::Schema.is_executable());
        assert!(!DocumentKind::Executable.is_schema());
        assert!(DocumentKind::Executable.is_executable());
    }

    #[test]
    fn test_file_content_creation() {
        let db = TestDatabase::default();
        let content: Arc<str> = Arc::from("type Query { hello: String }");
        let file_content = FileContent::new(&db, content);

        assert_eq!(
            file_content.text(&db).as_ref(),
            "type Query { hello: String }"
        );
    }

    #[test]
    fn test_file_metadata_creation() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let uri = FileUri::new("file:///test.graphql");

        let metadata = FileMetadata::new(
            &db,
            file_id,
            uri.clone(),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        assert_eq!(metadata.file_id(&db), file_id);
        assert_eq!(metadata.uri(&db), uri);
        assert_eq!(metadata.language(&db), Language::GraphQL);
        assert_eq!(metadata.document_kind(&db), DocumentKind::Schema);
        assert!(metadata.is_schema(&db));
        assert!(!metadata.is_document(&db));
        assert!(!metadata.requires_extraction(&db));
    }

    #[test]
    fn test_file_metadata_typescript() {
        let db = TestDatabase::default();
        let file_id = FileId::new(1);
        let uri = FileUri::new("file:///test.ts");

        let metadata = FileMetadata::new(
            &db,
            file_id,
            uri.clone(),
            Language::TypeScript,
            DocumentKind::Executable,
        );

        assert_eq!(metadata.language(&db), Language::TypeScript);
        assert_eq!(metadata.document_kind(&db), DocumentKind::Executable);
        assert!(!metadata.is_schema(&db));
        assert!(metadata.is_document(&db));
        assert!(metadata.requires_extraction(&db));
    }

    #[test]
    fn test_file_content_update() {
        let mut db = TestDatabase::default();
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
