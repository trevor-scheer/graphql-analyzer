//! File registry for mapping between file paths and database entities
//!
//! This module provides the bridge between editor file paths (strings/URIs)
//! and salsa database file identifiers.

use graphql_db::{
    DocumentFiles, FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles, SchemaFiles,
};
use salsa::Setter;
use std::collections::HashMap;
use std::sync::Arc;

use crate::FilePath;

/// Maps file paths to database file IDs and metadata
///
/// This is a temporary implementation for Phase 4. A more sophisticated
/// implementation will be added when we integrate with project configuration.
#[derive(Default)]
pub struct FileRegistry {
    next_id: u32,
    uri_to_id: HashMap<String, FileId>,
    id_to_uri: HashMap<FileId, String>,
    id_to_content: HashMap<FileId, FileContent>,
    id_to_metadata: HashMap<FileId, FileMetadata>,
    /// Separate input for schema files (enables fine-grained invalidation)
    schema_files: Option<SchemaFiles>,
    /// Separate input for document files (enables fine-grained invalidation)
    document_files: Option<DocumentFiles>,
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
    pub fn add_file<DB>(
        &mut self,
        db: &mut DB,
        path: &FilePath,
        content: &str,
        kind: FileKind,
        line_offset: u32,
    ) -> (FileId, FileContent, FileMetadata, bool)
    where
        DB: salsa::Database,
    {
        let uri_str = path.as_str();
        let content_arc: Arc<str> = Arc::from(content);

        // Check if file already exists
        if let Some(&existing_id) = self.uri_to_id.get(uri_str) {
            // File exists - update content in place using Salsa setter
            // This only invalidates queries that depend on this specific file's content
            if let Some(&existing_content) = self.id_to_content.get(&existing_id) {
                existing_content.set_text(db).to(content_arc);

                // Update metadata if needed (kind or line_offset changed)
                let metadata = self.id_to_metadata.get(&existing_id).copied().unwrap();
                if metadata.kind(db) != kind {
                    metadata.set_kind(db).to(kind);
                }
                if metadata.line_offset(db) != line_offset {
                    metadata.set_line_offset(db).to(line_offset);
                }

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
        if line_offset > 0 {
            metadata.set_line_offset(db).to(line_offset);
        }
        self.id_to_metadata.insert(file_id, metadata);

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
    pub fn rebuild_project_files<DB>(&mut self, db: &mut DB)
    where
        DB: salsa::Database,
    {
        let mut schema_file_list = Vec::new();
        let mut document_file_list = Vec::new();

        // Collect all file data first without calling db methods
        let file_data: Vec<_> = self
            .id_to_content
            .iter()
            .filter_map(|(&file_id, content)| {
                let metadata = self.id_to_metadata.get(&file_id)?;
                Some((file_id, *content, *metadata))
            })
            .collect();

        // Now query kinds and categorize - this may trigger salsa queries
        for (file_id, content, metadata) in file_data {
            let tuple = (file_id, content, metadata);

            match metadata.kind(db) {
                FileKind::Schema => schema_file_list.push(tuple),
                FileKind::ExecutableGraphQL | FileKind::TypeScript | FileKind::JavaScript => {
                    document_file_list.push(tuple);
                }
            }
        }

        // Create or update the SchemaFiles input
        let schema_files = if let Some(existing) = self.schema_files {
            existing.set_files(db).to(Arc::new(schema_file_list));
            existing
        } else {
            SchemaFiles::new(db, Arc::new(schema_file_list))
        };
        self.schema_files = Some(schema_files);

        // Create or update the DocumentFiles input
        let document_files = if let Some(existing) = self.document_files {
            existing.set_files(db).to(Arc::new(document_file_list));
            existing
        } else {
            DocumentFiles::new(db, Arc::new(document_file_list))
        };
        self.document_files = Some(document_files);

        // Create or update the ProjectFiles input
        let project_files = if let Some(existing) = self.project_files {
            // Update existing input to point to (potentially new) SchemaFiles/DocumentFiles
            existing.set_schema_files(db).to(schema_files);
            existing.set_document_files(db).to(document_files);
            existing
        } else {
            // Create new input
            ProjectFiles::new(db, schema_files, document_files)
        };

        self.project_files = Some(project_files);

        // ProjectFiles is now managed by the registry.
        // Queries that need ProjectFiles should accept it as a parameter.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::RootDatabase;

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
            0,
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
            0,
        );
        assert!(is_new1);

        // Update same file
        let (file_id2, _content2, _, is_new2) = registry.add_file(
            &mut db,
            &path,
            "type Query { world: String }",
            FileKind::Schema,
            0,
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
            0,
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
            0,
        );
        let (file_id2, _, _, _) = registry.add_file(
            &mut db,
            &path2,
            "type Mutation { update: Boolean }",
            FileKind::Schema,
            0,
        );

        let all_ids = registry.all_file_ids();
        assert_eq!(all_ids.len(), 2);
        assert!(all_ids.contains(&file_id1));
        assert!(all_ids.contains(&file_id2));
    }
}
