//! File registry for mapping between file paths and database entities
//!
//! This module provides the bridge between editor file paths (strings/URIs)
//! and salsa database file identifiers.
//!
//! ## Granular Caching Architecture
//!
//! The registry now uses `FileEntryMap` for true per-file granular caching:
//!
//! - Each file has its own `FileEntry` Salsa input
//! - When file A's content changes, only `FileEntry(A).content` is updated
//! - The `FileEntryMap` `HashMap` reference stays the same (same `Arc`)
//! - Queries for file B remain fully cached
//!
//! This is a significant improvement over the old `FileMap` approach where
//! any file change would invalidate queries for ALL files.

use graphql_base_db::{
    DocumentFileIds, FileContent, FileEntry, FileEntryMap, FileId, FileKind, FileMetadata, FileUri,
    ProjectFiles, SchemaFileIds,
};
use salsa::Setter;
use std::collections::HashMap;
use std::sync::Arc;

use crate::FilePath;

/// Maps file paths to database file IDs and metadata.
///
/// Implements granular per-file caching using `FileEntryMap`:
/// - Each file has its own `FileEntry` Salsa input
/// - Content updates only invalidate that specific file's queries
/// - The `HashMap` structure remains stable across content changes
#[derive(Default)]
pub struct FileRegistry {
    next_id: u32,
    uri_to_id: HashMap<String, FileId>,
    id_to_uri: HashMap<FileId, String>,
    id_to_content: HashMap<FileId, FileContent>,
    id_to_metadata: HashMap<FileId, FileMetadata>,
    /// Per-file `FileEntry` for granular invalidation
    id_to_entry: HashMap<FileId, FileEntry>,
    /// Granular input tracking schema file IDs only - changes on file add/remove
    schema_file_ids: Option<SchemaFileIds>,
    /// Granular input tracking document file IDs only - changes on file add/remove
    document_file_ids: Option<DocumentFileIds>,
    /// Per-file entry map for granular invalidation
    file_entry_map: Option<FileEntryMap>,
    /// The `ProjectFiles` input that tracks all files in the project
    project_files: Option<ProjectFiles>,
}

impl FileRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a file in the registry
    ///
    /// Returns the file ID, content, metadata, and a boolean indicating if this is a new file.
    /// If `is_new` is true, the caller should call `rebuild_project_files()` to update the index.
    /// If `is_new` is false (content-only update), rebuilding is NOT needed.
    ///
    /// ## Granular Caching
    ///
    /// For existing files, only the specific `FileContent.text` is updated. This means:
    /// - The `FileEntryMap` `HashMap` stays the same (no new `Arc` created)
    /// - Only queries depending on THIS file's content are invalidated
    /// - Queries for other files remain fully cached
    pub fn add_file<DB>(
        &mut self,
        db: &mut DB,
        path: &FilePath,
        content: &str,
        kind: FileKind,
    ) -> (FileId, FileContent, FileMetadata, bool)
    where
        DB: salsa::Database,
    {
        let uri_str = path.as_str();
        let content_arc: Arc<str> = Arc::from(content);

        // Check if file already exists
        if let Some(&existing_id) = self.uri_to_id.get(uri_str) {
            // File exists - only update if content or kind actually changed.
            // Salsa's input setters unconditionally bump the revision, so setting
            // identical content would invalidate all dependent queries and trigger
            // expensive recomputation (e.g., re-validating all 10k files when a
            // schema file is opened but not modified).
            if let Some(&existing_content) = self.id_to_content.get(&existing_id) {
                if *existing_content.text(db) != *content_arc {
                    existing_content.set_text(db).to(content_arc);
                }

                // Update metadata if needed (kind changed)
                let metadata = self.id_to_metadata.get(&existing_id).copied().unwrap();
                if metadata.kind(db) != kind {
                    metadata.set_kind(db).to(kind);
                }

                // Note: FileEntry is NOT updated here - it still points to the same
                // FileContent struct, which has been updated in-place. This is the key
                // to granular caching: the FileEntryMap HashMap doesn't change.

                return (existing_id, existing_content, metadata, false);
            }
        }

        // New file - create new FileId
        let file_id = FileId::new(self.next_id);
        self.next_id += 1;
        self.uri_to_id.insert(uri_str.to_string(), file_id);
        self.id_to_uri.insert(file_id, uri_str.to_string());

        // Create new FileContent
        let file_content = FileContent::new(db, content_arc);
        self.id_to_content.insert(file_id, file_content);

        // Create new FileMetadata
        let uri = FileUri::new(uri_str);
        let metadata = FileMetadata::new(db, file_id, uri, kind);
        self.id_to_metadata.insert(file_id, metadata);

        // Create new FileEntry (for granular caching)
        let file_entry = FileEntry::new(db, file_content, metadata);
        self.id_to_entry.insert(file_id, file_entry);

        (file_id, file_content, metadata, true)
    }

    /// Look up file ID by path
    #[must_use]
    pub fn get_file_id(&self, path: &FilePath) -> Option<FileId> {
        self.uri_to_id.get(path.as_str()).copied()
    }

    /// Look up path by file ID
    #[must_use]
    pub fn get_path(&self, file_id: FileId) -> Option<FilePath> {
        self.id_to_uri
            .get(&file_id)
            .map(|s| FilePath::new(s.clone()))
    }

    /// Get `FileContent` for a file ID
    #[must_use]
    pub fn get_content(&self, file_id: FileId) -> Option<FileContent> {
        self.id_to_content.get(&file_id).copied()
    }

    /// Get `FileMetadata` for a file ID
    #[must_use]
    pub fn get_metadata(&self, file_id: FileId) -> Option<FileMetadata> {
        self.id_to_metadata.get(&file_id).copied()
    }

    /// Remove a file from the registry
    pub fn remove_file(&mut self, file_id: FileId) {
        if let Some(uri) = self.id_to_uri.remove(&file_id) {
            self.uri_to_id.remove(&uri);
        }
        self.id_to_content.remove(&file_id);
        self.id_to_metadata.remove(&file_id);
    }

    /// Get all file IDs
    #[must_use]
    pub fn all_file_ids(&self) -> Vec<FileId> {
        self.id_to_uri.keys().copied().collect()
    }

    /// Get the `ProjectFiles` input
    #[must_use]
    pub const fn project_files(&self) -> Option<ProjectFiles> {
        self.project_files
    }

    /// Rebuild the `ProjectFiles` input from current state
    /// This should be called after files are added or removed
    ///
    /// Note: This method should be called WITHOUT holding any locks to avoid deadlocks
    ///
    /// ## Granular Caching
    ///
    /// This method creates/updates the `FileEntryMap` which enables granular per-file caching.
    /// The key insight is that when a file's content changes later (via `add_file` with `is_new=false`),
    /// we only update the `FileContent.text` field - NOT the `FileEntryMap` `HashMap`.
    /// This means the `HashMap` `Arc` stays the same, and queries for other files remain cached.
    pub fn rebuild_project_files<DB>(&mut self, db: &mut DB)
    where
        DB: salsa::Database,
    {
        let mut schema_ids = Vec::new();
        let mut document_ids = Vec::new();
        let mut file_entries: HashMap<FileId, FileEntry> = HashMap::new();

        // Collect all file data first without calling db methods
        let file_data: Vec<_> = self
            .id_to_content
            .iter()
            .filter_map(|(&file_id, _content)| {
                let metadata = self.id_to_metadata.get(&file_id)?;
                let entry = self.id_to_entry.get(&file_id)?;
                Some((file_id, *metadata, *entry))
            })
            .collect();

        // Now query kinds and categorize - this may trigger salsa queries
        for (file_id, metadata, entry) in file_data {
            file_entries.insert(file_id, entry);

            // Categorize by kind for ID lists
            let kind = metadata.kind(db);
            if kind.is_schema() {
                schema_ids.push(file_id);
            } else if kind.is_document() {
                document_ids.push(file_id);
            }
        }

        // Create or update the SchemaFileIds input
        // Only update if the IDs have actually changed to avoid invalidating queries
        let schema_file_ids = if let Some(existing) = self.schema_file_ids {
            let existing_ids = existing.ids(db);
            if existing_ids.as_slice() != schema_ids.as_slice() {
                existing.set_ids(db).to(Arc::new(schema_ids));
            }
            existing
        } else {
            SchemaFileIds::new(db, Arc::new(schema_ids))
        };
        self.schema_file_ids = Some(schema_file_ids);

        // Create or update the DocumentFileIds input
        // Only update if the IDs have actually changed to avoid invalidating queries
        let document_file_ids = if let Some(existing) = self.document_file_ids {
            let existing_ids = existing.ids(db);
            if existing_ids.as_slice() != document_ids.as_slice() {
                existing.set_ids(db).to(Arc::new(document_ids));
            }
            existing
        } else {
            DocumentFileIds::new(db, Arc::new(document_ids))
        };
        self.document_file_ids = Some(document_file_ids);

        // Create or update the FileEntryMap input
        // Only update if the set of files has changed (entries point to same FileEntry objects)
        let file_entry_map = if let Some(existing) = self.file_entry_map {
            let existing_entries = existing.entries(db);
            // Compare file IDs only - the FileEntry values are the same objects
            let keys_match = existing_entries.len() == file_entries.len()
                && existing_entries
                    .keys()
                    .all(|k| file_entries.contains_key(k));
            if !keys_match {
                existing.set_entries(db).to(Arc::new(file_entries));
            }
            existing
        } else {
            FileEntryMap::new(db, Arc::new(file_entries))
        };
        self.file_entry_map = Some(file_entry_map);

        // Create or update the ProjectFiles input
        // Only update child references if they actually changed
        if let Some(existing) = self.project_files {
            if existing.schema_file_ids(db) != schema_file_ids {
                existing.set_schema_file_ids(db).to(schema_file_ids);
            }
            if existing.document_file_ids(db) != document_file_ids {
                existing.set_document_file_ids(db).to(document_file_ids);
            }
            if existing.file_entry_map(db) != file_entry_map {
                existing.set_file_entry_map(db).to(file_entry_map);
            }
            self.project_files = Some(existing);
        } else {
            self.project_files = Some(ProjectFiles::new(
                db,
                schema_file_ids,
                document_file_ids,
                file_entry_map,
            ));
        }
    }

    /// Get the `FileEntry` for a file ID
    #[must_use]
    pub fn get_entry(&self, file_id: FileId) -> Option<FileEntry> {
        self.id_to_entry.get(&file_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_ide_db::RootDatabase;

    #[test]
    fn test_file_registry_add_and_lookup() {
        let mut db = RootDatabase::new();
        let mut registry = FileRegistry::new();

        let path = FilePath::new("file:///test.graphql");
        let (file_id, _content, _metadata, is_new) = registry.add_file(
            &mut db,
            &path,
            "type Query { hello: String }",
            FileKind::Schema,
        );

        // Should indicate this is a new file
        assert!(is_new);

        // Should be able to look up by path
        assert_eq!(registry.get_file_id(&path), Some(file_id));

        // Should be able to look up by file ID
        assert_eq!(registry.get_path(file_id), Some(path.clone()));

        // Should have content and metadata
        assert!(registry.get_content(file_id).is_some());
        assert!(registry.get_metadata(file_id).is_some());
    }

    #[test]
    fn test_file_registry_update_existing() {
        let mut db = RootDatabase::new();
        let mut registry = FileRegistry::new();

        let path = FilePath::new("file:///test.graphql");

        // Add file
        let (file_id1, _, _, is_new1) = registry.add_file(
            &mut db,
            &path,
            "type Query { hello: String }",
            FileKind::Schema,
        );
        assert!(is_new1);

        // Update same file
        let (file_id2, _content2, _, is_new2) = registry.add_file(
            &mut db,
            &path,
            "type Query { world: String }",
            FileKind::Schema,
        );

        // Should indicate this is NOT a new file (just an update)
        assert!(!is_new2);

        // Should reuse the same file ID
        assert_eq!(file_id1, file_id2);

        // Content should be updated
        let updated_content = registry.get_content(file_id2).unwrap();
        assert_eq!(
            updated_content.text(&db).as_ref(),
            "type Query { world: String }"
        );
    }

    #[test]
    fn test_file_registry_remove() {
        let mut db = RootDatabase::new();
        let mut registry = FileRegistry::new();

        let path = FilePath::new("file:///test.graphql");
        let (file_id, _, _, _) = registry.add_file(
            &mut db,
            &path,
            "type Query { hello: String }",
            FileKind::Schema,
        );

        // Remove the file
        registry.remove_file(file_id);

        // Should no longer be found
        assert_eq!(registry.get_file_id(&path), None);
        assert_eq!(registry.get_path(file_id), None);
    }

    #[test]
    fn test_file_registry_all_files() {
        let mut db = RootDatabase::new();
        let mut registry = FileRegistry::new();

        let path1 = FilePath::new("file:///test1.graphql");
        let path2 = FilePath::new("file:///test2.graphql");

        let (file_id1, _, _, _) = registry.add_file(
            &mut db,
            &path1,
            "type Query { hello: String }",
            FileKind::Schema,
        );
        let (file_id2, _, _, _) = registry.add_file(
            &mut db,
            &path2,
            "type Mutation { update: Boolean }",
            FileKind::Schema,
        );

        let all_ids = registry.all_file_ids();
        assert_eq!(all_ids.len(), 2);
        assert!(all_ids.contains(&file_id1));
        assert!(all_ids.contains(&file_id2));
    }
}
