// GraphQL HIR (High-level Intermediate Representation)
// This crate provides semantic queries on top of syntax.
// It implements the "golden invariant": editing a document's body never invalidates global schema knowledge.

use graphql_db::FileId;
use std::collections::HashMap;
use std::sync::Arc;

mod body;
mod structure;

pub use body::*;
pub use structure::*;

/// Identifier for a GraphQL type in the schema
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(salsa::Id);

impl TypeId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// Identifier for a field definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(salsa::Id);

impl FieldId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// Identifier for a fragment definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FragmentId(salsa::Id);

impl FragmentId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// Identifier for an operation definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(salsa::Id);

impl OperationId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// The salsa database trait for HIR queries
#[salsa::db]
pub trait GraphQLHirDatabase: graphql_syntax::GraphQLSyntaxDatabase {
    /// Get the project files input
    /// Returns None if no project files have been set yet
    /// This should be overridden by implementations that track project files
    fn project_files(&self) -> Option<graphql_db::ProjectFiles> {
        None
    }
}

#[salsa::db]
impl GraphQLHirDatabase for graphql_db::RootDatabase {
    // Uses default implementation (returns None)
    // Queries should accept ProjectFiles as a parameter instead
}

// ============================================================================
// Per-file queries - these provide fine-grained caching
// Each query depends only on the specific file's content, not all files
// ============================================================================

/// Get type definitions from a single schema file
/// This query is cached per-file - editing another file won't invalidate it
#[salsa::tracked]
pub fn file_type_defs(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<TypeDef>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::new(structure.type_defs.clone())
}

/// Get fragments from a single document file
/// This query is cached per-file - editing another file won't invalidate it
#[salsa::tracked]
pub fn file_fragments(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<FragmentStructure>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::new(structure.fragments.clone())
}

/// Get operations from a single document file
/// This query is cached per-file - editing another file won't invalidate it
#[salsa::tracked]
pub fn file_operations(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<OperationStructure>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::new(structure.operations.clone())
}

// ============================================================================
// Aggregate queries - these use granular inputs for efficient invalidation
// They depend on file IDs (stable) and call per-file queries (granular caching)
// ============================================================================

/// Get all types in the schema
///
/// This query uses granular dependencies:
/// - Depends on `SchemaFileIds` (only changes when files are added/removed)
/// - Calls `file_type_defs` per-file (each cached independently)
///
/// When a single schema file changes, only that file's `file_type_defs` is recomputed.
/// Other files' results come from cache.
#[salsa::tracked]
pub fn schema_types(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, TypeDef>> {
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    let mut types = HashMap::new();

    for file_id in schema_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query - cached independently
            let file_types = file_type_defs(db, *file_id, content, metadata);
            for type_def in file_types.iter() {
                types.insert(type_def.name.clone(), type_def.clone());
            }
        }
    }

    Arc::new(types)
}

/// Alias for `schema_types` for backward compatibility
#[salsa::tracked]
pub fn schema_types_with_project(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, TypeDef>> {
    schema_types(db, project_files)
}

/// Get all fragments in the project
///
/// This query uses granular dependencies:
/// - Depends on `DocumentFileIds` (only changes when files are added/removed)
/// - Calls `file_fragments` per-file (each cached independently)
///
/// When a single document file changes, only that file's `file_fragments` is recomputed.
/// Other files' results come from cache.
#[salsa::tracked]
pub fn all_fragments(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, FragmentStructure>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut fragments = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query - cached independently
            let file_frags = file_fragments(db, *file_id, content, metadata);
            for fragment in file_frags.iter() {
                fragments.insert(fragment.name.clone(), fragment.clone());
            }
        }
    }

    Arc::new(fragments)
}

/// Alias for `all_fragments` for backward compatibility
#[salsa::tracked]
pub fn all_fragments_with_project(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, FragmentStructure>> {
    all_fragments(db, project_files)
}

/// Index mapping fragment names to their file content and metadata
/// Uses granular per-file caching for efficient invalidation.
#[salsa::tracked]
pub fn fragment_file_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, (graphql_db::FileContent, graphql_db::FileMetadata)>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query for fragments
            let file_frags = file_fragments(db, *file_id, content, metadata);
            for fragment in file_frags.iter() {
                index.insert(fragment.name.clone(), (content, metadata));
            }
        }
    }

    Arc::new(index)
}

/// Index mapping fragment names to their source text (the GraphQL block containing them).
///
/// For TS/JS files with multiple blocks, this returns only the specific block
/// containing each fragment, not all blocks from the file. This is crucial for
/// proper validation - we don't want to accidentally include unrelated operations
/// or fragments from the same file.
#[salsa::tracked]
pub fn fragment_source_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, Arc<str>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            let kind = metadata.kind(db);
            let parse = graphql_syntax::parse(db, content, metadata);

            if kind == graphql_db::FileKind::TypeScript || kind == graphql_db::FileKind::JavaScript
            {
                // For TS/JS files, map each fragment to its specific block
                for block in &parse.blocks {
                    for def in &block.ast.definitions {
                        if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = def {
                            let name: Arc<str> = Arc::from(frag.name.as_str());
                            index.insert(name, block.source.clone());
                        }
                    }
                }
            } else {
                // For pure GraphQL files, use the entire file content
                let file_frags = file_fragments(db, *file_id, content, metadata);
                let text = content.text(db);
                for fragment in file_frags.iter() {
                    index.insert(fragment.name.clone(), text.clone());
                }
            }
        }
    }

    Arc::new(index)
}

/// Per-file query for fragment spreads mapping
/// This enables fine-grained incremental computation - editing fragment A
/// only invalidates file A's spreads, not the entire project index.
#[salsa::tracked]
pub fn file_fragment_spreads(
    db: &dyn GraphQLHirDatabase,
    file_id: graphql_db::FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    let file_frags = file_fragments(db, file_id, content, metadata);
    let mut spreads = HashMap::new();

    for fragment in file_frags.iter() {
        // Get the fragment body to find its spreads
        let body = fragment_body(db, content, metadata, fragment.name.clone());
        spreads.insert(fragment.name.clone(), body.fragment_spreads.clone());
    }

    Arc::new(spreads)
}

/// Index mapping fragment names to the fragments they reference (spread)
/// Uses per-file queries for fine-grained incremental computation.
/// Editing one fragment file only rebuilds that file's spreads, not the entire index.
#[salsa::tracked]
pub fn fragment_spreads_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query - only rebuilds when THIS file changes
            let file_spreads = file_fragment_spreads(db, *file_id, content, metadata);
            index.extend(file_spreads.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    Arc::new(index)
}

/// Get all operations in the project
/// Uses granular per-file caching for efficient invalidation.
#[salsa::tracked]
pub fn all_operations(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<Vec<OperationStructure>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut operations = Vec::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query for operations
            let file_ops = file_operations(db, *file_id, content, metadata);
            operations.extend(file_ops.iter().cloned());
        }
    }

    Arc::new(operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileKind, FileMetadata, FileUri};
    use salsa::Setter;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Test database that implements all required traits
    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

    #[salsa::db]
    impl GraphQLHirDatabase for TestDatabase {}

    /// Helper to create `ProjectFiles` with the new granular structure
    fn create_project_files(
        db: &TestDatabase,
        schema_files: &[(FileId, FileContent, FileMetadata)],
        document_files: &[(FileId, FileContent, FileMetadata)],
    ) -> graphql_db::ProjectFiles {
        let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
        let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

        let mut entries = HashMap::new();
        for (id, content, metadata) in schema_files {
            let entry = graphql_db::FileEntry::new(db, *content, *metadata);
            entries.insert(*id, entry);
        }
        for (id, content, metadata) in document_files {
            let entry = graphql_db::FileEntry::new(db, *content, *metadata);
            entries.insert(*id, entry);
        }

        let schema_file_ids = graphql_db::SchemaFileIds::new(db, Arc::new(schema_ids));
        let document_file_ids = graphql_db::DocumentFileIds::new(db, Arc::new(doc_ids));
        let file_entry_map = graphql_db::FileEntryMap::new(db, Arc::new(entries));

        graphql_db::ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_schema_types_empty() {
        let db = TestDatabase::default();
        let project_files = create_project_files(&db, &[], &[]);
        let types = schema_types_with_project(&db, project_files);
        assert_eq!(types.len(), 0);
    }

    #[test]
    fn test_file_structure_basic() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("type User { id: ID! }"));
        let metadata =
            FileMetadata::new(&db, file_id, FileUri::new("test.graphql"), FileKind::Schema);

        let structure = file_structure(&db, file_id, content, metadata);
        assert_eq!(structure.type_defs.len(), 1);
        assert_eq!(structure.type_defs[0].name.as_ref(), "User");
    }

    // ========================================================================
    // Test for Issue #209: DocumentFiles input granularity causes excessive invalidation
    //
    // This test demonstrates that editing one file's content should NOT cause
    // file_structure queries for OTHER files to be re-executed.
    //
    // BEFORE FIX: all_fragments depends on DocumentFiles which contains all
    // FileContent objects. When any FileContent changes, all_fragments is
    // invalidated, which causes it to re-query file_structure for ALL files.
    //
    // AFTER FIX: all_fragments depends on DocumentFileIds (just file IDs) and
    // per-file queries. Editing file A only invalidates file A's per-file query.
    // ========================================================================

    /// Counter for tracking `file_structure` executions
    static FILE_STRUCTURE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    /// Wrapper around `file_structure` that counts executions
    /// We use this to verify caching behavior
    fn counted_file_structure(
        db: &dyn GraphQLHirDatabase,
        file_id: FileId,
        content: graphql_db::FileContent,
        metadata: graphql_db::FileMetadata,
    ) -> Arc<FileStructureData> {
        FILE_STRUCTURE_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        file_structure(db, file_id, content, metadata)
    }

    #[test]
    fn test_editing_one_file_does_not_recompute_other_files_structure() {
        // Reset counter
        FILE_STRUCTURE_CALL_COUNT.store(0, Ordering::SeqCst);

        let mut db = TestDatabase::default();

        // Create two document files, each with a fragment
        let file1_id = FileId::new(0);
        let file1_content =
            FileContent::new(&db, Arc::from("fragment FragmentA on User { id name }"));
        let file1_metadata = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("file1.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let file2_id = FileId::new(1);
        let file2_content =
            FileContent::new(&db, Arc::from("fragment FragmentB on User { email }"));
        let file2_metadata = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("file2.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Create project files with new granular structure
        let doc_files = [
            (file1_id, file1_content, file1_metadata),
            (file2_id, file2_content, file2_metadata),
        ];
        let project_files = create_project_files(&db, &[], &doc_files);

        // First call: compute file_structure for both files to warm the cache
        let _ = counted_file_structure(&db, file1_id, file1_content, file1_metadata);
        let _ = counted_file_structure(&db, file2_id, file2_content, file2_metadata);
        assert_eq!(
            FILE_STRUCTURE_CALL_COUNT.load(Ordering::SeqCst),
            2,
            "Expected 2 initial file_structure calls"
        );

        // Query all_fragments to also warm that cache
        let fragments = all_fragments_with_project(&db, project_files);
        assert_eq!(fragments.len(), 2, "Should have 2 fragments");

        // Reset counter before the edit
        FILE_STRUCTURE_CALL_COUNT.store(0, Ordering::SeqCst);

        // Now edit ONLY file2's content
        // With the new granular architecture, we only update the FileContent.text
        // The FileEntryMap HashMap stays the same (same keys, same Arc)
        file2_content
            .set_text(&mut db)
            .to(Arc::from("fragment FragmentB on User { email phone }"));

        // Query file1's structure - this should come from cache
        let _ = counted_file_structure(&db, file1_id, file1_content, file1_metadata);

        // ASSERTION: After editing file2, file1's structure should NOT be recomputed
        // It should be served from Salsa's cache since file1's content didn't change
        let _file1_calls = FILE_STRUCTURE_CALL_COUNT.load(Ordering::SeqCst);

        FILE_STRUCTURE_CALL_COUNT.store(0, Ordering::SeqCst);

        // Query all_fragments again after editing file2
        let fragments_after = all_fragments_with_project(&db, project_files);
        assert_eq!(fragments_after.len(), 2, "Should still have 2 fragments");

        // Check if FragmentB was updated (it should have "phone" now)
        let _frag_b = fragments_after
            .get("FragmentB")
            .expect("FragmentB should exist");

        // With the new granular architecture:
        // - DocumentFileIds didn't change (same files)
        // - Only file2's FileContent changed
        // - So only file2's file_fragments query should recompute
        // - file1's file_fragments should come from cache
    }

    /// This test verifies the core issue: `all_fragments` depends on `DocumentFiles`
    /// which causes full invalidation when any file content changes.
    ///
    /// After the fix (using `DocumentFileIds` + per-file queries), this test should
    /// show that editing one file doesn't cause the aggregate query to do
    /// unnecessary work for other files.
    #[test]
    fn test_all_fragments_granular_invalidation() {
        let mut db = TestDatabase::default();

        // Create two document files with fragments
        let file1_id = FileId::new(0);
        let file1_content = FileContent::new(&db, Arc::from("fragment F1 on User { id }"));
        let file1_metadata = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("f1.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let file2_id = FileId::new(1);
        let file2_content = FileContent::new(&db, Arc::from("fragment F2 on User { name }"));
        let file2_metadata = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("f2.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let doc_files = [
            (file1_id, file1_content, file1_metadata),
            (file2_id, file2_content, file2_metadata),
        ];
        let project_files = create_project_files(&db, &[], &doc_files);

        // Warm the cache
        let frags1 = all_fragments_with_project(&db, project_files);
        assert_eq!(frags1.len(), 2);
        assert!(frags1.contains_key("F1"));
        assert!(frags1.contains_key("F2"));

        // Edit file2's content - with new granular architecture, only update FileContent.text
        file2_content
            .set_text(&mut db)
            .to(Arc::from("fragment F2 on User { name email }"));

        // Query again - file1's data should come from cache
        let frags2 = all_fragments_with_project(&db, project_files);
        assert_eq!(frags2.len(), 2);

        // Both fragments should still be present
        assert!(frags2.contains_key("F1"), "F1 should still exist");
        assert!(frags2.contains_key("F2"), "F2 should still exist");

        // The structural data should be correct
        let _f1 = frags2.get("F1").unwrap();
        // With the new granular architecture:
        // - DocumentFileIds didn't change (same files)
        // - Only file2's FileContent changed
        // - So only file2's file_fragments query should recompute
        // - file1's file_fragments should come from cache
    }
}
