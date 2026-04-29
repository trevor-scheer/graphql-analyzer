//! Salsa-backed view over project files for IDE feature modules.
//!
//! Replaces the old `&FileRegistry` parameter that feature modules took. Every
//! lookup goes through `&dyn salsa::Database` and the `FilePathMap` /
//! `FileEntryMap` salsa inputs, so `Analysis` snapshots no longer need to
//! reach back into the host through a `parking_lot` `RwLock` to resolve URIs.
//!
//! This is what makes the deadlock cycle structurally impossible: there is no
//! second lock that a snapshot can park on while the host's Salsa setter waits
//! for the snapshot to drop.

use std::sync::Arc;

use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};

use crate::FilePath;

/// Read-only view over the project's files, backed entirely by Salsa.
///
/// Exposes the same `get_file_id` / `get_path` / `get_content` / `get_metadata`
/// / `all_file_ids` surface that feature modules previously accessed via
/// `&FileRegistry`. Cheap to construct and `Copy`.
#[derive(Copy, Clone)]
pub struct DbFiles<'a> {
    db: &'a dyn salsa::Database,
    project_files: Option<ProjectFiles>,
}

impl<'a> DbFiles<'a> {
    pub fn new(db: &'a dyn salsa::Database, project_files: Option<ProjectFiles>) -> Self {
        Self { db, project_files }
    }

    pub fn get_file_id(&self, path: &FilePath) -> Option<FileId> {
        let pf = self.project_files?;
        let uri: Arc<str> = Arc::from(path.as_str());
        graphql_base_db::file_id_for_uri(self.db, pf, uri)
    }

    pub fn get_path(&self, file_id: FileId) -> Option<FilePath> {
        let pf = self.project_files?;
        let uri = graphql_base_db::uri_for_file_id(self.db, pf, file_id)?;
        Some(FilePath::new(uri.as_ref().to_string()))
    }

    pub fn get_content(&self, file_id: FileId) -> Option<FileContent> {
        let pf = self.project_files?;
        graphql_base_db::file_lookup(self.db, pf, file_id).map(|(c, _)| c)
    }

    pub fn get_metadata(&self, file_id: FileId) -> Option<FileMetadata> {
        let pf = self.project_files?;
        graphql_base_db::file_lookup(self.db, pf, file_id).map(|(_, m)| m)
    }

    pub fn all_file_ids(&self) -> Vec<FileId> {
        match self.project_files {
            Some(pf) => graphql_base_db::all_file_ids(self.db, pf).as_ref().clone(),
            None => Vec::new(),
        }
    }
}
