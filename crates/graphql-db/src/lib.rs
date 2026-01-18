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

impl FileKind {
    /// Returns true if this is a schema file
    #[must_use]
    pub const fn is_schema(self) -> bool {
        matches!(self, Self::Schema)
    }

    /// Returns true if this is a document file (operations/fragments)
    ///
    /// This includes pure GraphQL executable files and TypeScript/JavaScript
    /// files with embedded GraphQL.
    #[must_use]
    pub const fn is_document(self) -> bool {
        matches!(
            self,
            Self::ExecutableGraphQL | Self::TypeScript | Self::JavaScript
        )
    }
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

/// Query tracking for testing Salsa caching behavior
///
/// This module provides utilities for verifying that Salsa's incremental computation
/// is working correctly. It uses Salsa's event callback mechanism to track when queries
/// are actually executed vs served from cache.
///
/// ## Design Decisions (based on SME review)
///
/// 1. **Per-database tracking**: Each `TrackedDatabase` has its own query log, avoiding
///    global state and parallel test interference.
///
/// 2. **Simple threading**: Uses `Mutex<HashMap<String, usize>>` instead of redundant
///    `Mutex<HashMap<String, AtomicUsize>>` - atomics provide no benefit when behind a mutex.
///
/// 3. **Checkpoint-based assertions**: Tests use `checkpoint()` and `count_since()` for
///    deterministic assertions without needing to reset global state.
///
/// 4. **Query name constants**: Prevents typos in test assertions.
///
/// ## Usage
///
/// ```ignore
/// use graphql_db::tracking::{TrackedDatabase, queries};
///
/// let mut db = TrackedDatabase::new();
/// // ... setup files ...
///
/// // Take a checkpoint before the operation we want to measure
/// let checkpoint = db.checkpoint();
///
/// // Call some queries
/// let result = schema_types(&db, project_files);
///
/// // Assert on executions since checkpoint
/// assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint), 1);
///
/// // Second call should be cached
/// let checkpoint2 = db.checkpoint();
/// let result2 = schema_types(&db, project_files);
/// assert_eq!(db.count_since(queries::SCHEMA_TYPES, checkpoint2), 0); // Cached!
/// ```
#[cfg(any(test, feature = "test-utils"))]
pub mod tracking {
    use salsa::{Event, EventKind, Storage};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Well-known query names to prevent typos in test assertions.
    ///
    /// These match the function names of Salsa tracked queries.
    /// NOTE: Query name extraction relies on Salsa's Debug format being `query_name(...)`.
    /// If tests fail after a Salsa upgrade, verify the debug format hasn't changed.
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
        // Per-file contribution queries for project-wide lint rules
        pub const FILE_USED_FRAGMENT_NAMES: &str = "file_used_fragment_names";
        pub const FILE_DEFINED_FRAGMENT_NAMES: &str = "file_defined_fragment_names";
        pub const FILE_OPERATION_NAMES: &str = "file_operation_names";
        pub const FILE_SCHEMA_COORDINATES: &str = "file_schema_coordinates";
    }

    /// Per-database query execution log.
    ///
    /// This is stored inside `TrackedDatabase` and tracks all `WillExecute` events
    /// for that specific database instance. No global state means no parallel test
    /// interference.
    #[derive(Default)]
    struct QueryLog {
        /// Ordered log of query executions
        executions: Vec<String>,
        /// Aggregated counts by query name
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

    /// Extracts the query name from Salsa's debug representation.
    ///
    /// The debug format is like `query_name(args...)` or `module::query_name(args...)`.
    /// We extract just the function name without module path or arguments.
    fn extract_query_name(database_key: &dyn std::fmt::Debug) -> String {
        let debug_str = format!("{database_key:?}");
        let without_args = debug_str.split('(').next().unwrap_or(&debug_str);
        without_args
            .rsplit("::")
            .next()
            .unwrap_or(without_args)
            .to_string()
    }

    /// A Salsa database that tracks query executions.
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
        /// Create a new tracked database with event tracking enabled.
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

        /// Helper to acquire the log lock and run a closure.
        fn with_log<F, R>(&self, f: F) -> R
        where
            F: FnOnce(&QueryLog) -> R,
        {
            f(&self.log.lock().expect("QueryLog mutex poisoned"))
        }

        /// Get the current checkpoint (log position) for later comparison.
        ///
        /// Use this before an operation, then use `count_since()` after to measure
        /// how many queries executed.
        #[must_use]
        pub fn checkpoint(&self) -> usize {
            self.with_log(QueryLog::checkpoint)
        }

        /// Count executions of a specific query since the given checkpoint.
        ///
        /// This is the primary way to assert on caching behavior:
        /// - `count_since(queries::PARSE, checkpoint) == 1` means parse ran once
        /// - `count_since(queries::PARSE, checkpoint) == 0` means it was cached
        #[must_use]
        pub fn count_since(&self, query_name: &str, checkpoint: usize) -> usize {
            self.with_log(|log| log.count_since(query_name, checkpoint))
        }

        /// Get all query executions since the given checkpoint.
        ///
        /// Useful for debugging test failures - shows exactly what executed.
        #[must_use]
        pub fn executions_since(&self, checkpoint: usize) -> Vec<String> {
            self.with_log(|log| log.executions_since(checkpoint))
        }

        /// Get total execution count for a query (since database creation or last reset).
        #[must_use]
        pub fn total_count(&self, query_name: &str) -> usize {
            self.with_log(|log| log.total_count(query_name))
        }

        /// Get all query counts (since database creation or last reset).
        #[must_use]
        pub fn all_counts(&self) -> HashMap<String, usize> {
            self.with_log(QueryLog::all_counts)
        }

        /// Reset all tracking data. Generally prefer checkpoint-based assertions instead.
        pub fn reset(&self) {
            self.log.lock().expect("QueryLog mutex poisoned").reset();
        }
    }

    #[salsa::db]
    impl salsa::Database for TrackedDatabase {}

    // SAFETY: storage/storage_mut return references to the owned storage field
    unsafe impl salsa::plumbing::HasStorage for TrackedDatabase {
        fn storage(&self) -> &Storage<Self> {
            &self.storage
        }

        fn storage_mut(&mut self) -> &mut Storage<Self> {
            &mut self.storage
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_query_name_extraction() {
            struct MockKey(&'static str);
            impl std::fmt::Debug for MockKey {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }

            assert_eq!(
                extract_query_name(&MockKey("parse(FileContent(...))")),
                "parse"
            );
            assert_eq!(
                extract_query_name(&MockKey("graphql_hir::schema_types(ProjectFiles(...))")),
                "schema_types"
            );
            assert_eq!(extract_query_name(&MockKey("simple")), "simple");
        }

        #[test]
        fn test_checkpoint_based_counting() {
            let log = Arc::new(Mutex::new(QueryLog::default()));

            {
                let mut l = log.lock().unwrap();
                l.record("parse");
                l.record("parse");
                l.record("schema_types");
            }

            let checkpoint = log.lock().unwrap().checkpoint();
            assert_eq!(checkpoint, 3);

            {
                let mut l = log.lock().unwrap();
                l.record("parse");
            }

            assert_eq!(log.lock().unwrap().count_since("parse", checkpoint), 1);
            assert_eq!(log.lock().unwrap().count_since("parse", 0), 3);
            assert_eq!(
                log.lock().unwrap().count_since("schema_types", checkpoint),
                0
            );
        }
    }
}

/// Test utilities for creating project files
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    use super::{
        DocumentFileIds, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata, ProjectFiles,
        SchemaFileIds,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Helper to create `ProjectFiles` for tests
    ///
    /// This function takes lists of schema and document files and creates
    /// the proper granular Salsa inputs (`SchemaFileIds`, `DocumentFileIds`, `FileEntryMap`).
    ///
    /// Uses `FileEntryMap` for per-file granular caching.
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
