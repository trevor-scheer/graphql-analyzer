//! Workspace management for the GraphQL Language Server.
//!
//! This module provides the `WorkspaceManager` struct which manages:
//! - Workspace folder tracking
//! - Configuration paths and loaded configs
//! - AnalysisHost instances per workspace/project
//! - File-to-project mapping for efficient lookups
//!
//! ## Architecture
//!
//! The workspace manager separates concerns:
//! - **Server**: Handles LSP protocol messages
//! - **WorkspaceManager**: Manages workspace state and project data
//! - **AnalysisHost** (in graphql-ide): Handles IDE features for a single project

use dashmap::DashMap;
use graphql_ide::AnalysisHost;
use lsp_types::Uri;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp_server::UriExt;

/// Manages workspace state for the GraphQL Language Server.
///
/// This struct holds all per-workspace and per-project data:
/// - Workspace folder paths
/// - Configuration file paths and loaded configs
/// - AnalysisHost instances (one per project)
/// - Document version tracking
/// - File-to-project mapping
pub struct WorkspaceManager {
    /// Workspace folders from initialization (stored temporarily until configs are loaded)
    pub init_workspace_folders: DashMap<String, PathBuf>,

    /// Workspace roots indexed by workspace folder URI string
    pub workspace_roots: DashMap<String, PathBuf>,

    /// Config file paths indexed by workspace URI string
    pub config_paths: DashMap<String, PathBuf>,

    /// Loaded GraphQL configs indexed by workspace URI string
    pub configs: DashMap<String, graphql_config::GraphQLConfig>,

    /// `AnalysisHost` per (workspace URI, project name) tuple
    pub hosts: DashMap<(String, String), Arc<Mutex<AnalysisHost>>>,

    /// Document versions indexed by document URI string
    /// Used to detect out-of-order updates and avoid race conditions
    pub document_versions: DashMap<String, i32>,

    /// Reverse index: file URI → (`workspace_uri`, `project_name`)
    /// Provides O(1) lookup instead of O(n) iteration over all hosts
    pub file_to_project: DashMap<String, (String, String)>,
}

impl WorkspaceManager {
    /// Create a new workspace manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            init_workspace_folders: DashMap::new(),
            workspace_roots: DashMap::new(),
            config_paths: DashMap::new(),
            configs: DashMap::new(),
            hosts: DashMap::new(),
            document_versions: DashMap::new(),
            file_to_project: DashMap::new(),
        }
    }

    /// Get or create an `AnalysisHost` for a workspace/project
    pub fn get_or_create_host(
        &self,
        workspace_uri: &str,
        project_name: &str,
    ) -> Arc<Mutex<AnalysisHost>> {
        self.hosts
            .entry((workspace_uri.to_string(), project_name.to_string()))
            .or_insert_with(|| Arc::new(Mutex::new(AnalysisHost::new())))
            .clone()
    }

    /// Find the workspace and project for a given document URI
    ///
    /// Uses a reverse index for O(1) lookup of previously seen files.
    /// Falls back to config pattern matching for files opened after init
    /// that haven't been indexed yet.
    pub fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
        let uri_string = document_uri.to_string();

        // First, check the reverse index
        if let Some(entry) = self.file_to_project.get(&uri_string) {
            return Some(entry.value().clone());
        }

        // Fall back to searching configs for pattern matching
        let doc_path = document_uri.to_file_path()?;
        for workspace_entry in self.workspace_roots.iter() {
            let workspace_uri = workspace_entry.key();
            let workspace_path = workspace_entry.value();

            if doc_path.as_ref().starts_with(workspace_path.as_path()) {
                if let Some(config) = self.configs.get(workspace_uri.as_str()) {
                    if let Some(project_name) =
                        config.find_project_for_document(&doc_path, workspace_path)
                    {
                        return Some((workspace_uri.clone(), project_name.to_string()));
                    }
                }
                return None;
            }
        }

        None
    }

    /// Register a file in the file-to-project index
    #[allow(dead_code)]
    pub fn register_file(&self, file_uri: &str, workspace_uri: &str, project_name: &str) {
        self.file_to_project.insert(
            file_uri.to_string(),
            (workspace_uri.to_string(), project_name.to_string()),
        );
    }

    /// Clear all state for a workspace
    ///
    /// Used when reloading configuration.
    #[allow(dead_code)]
    pub fn clear_workspace(&self, workspace_uri: &str) {
        // Remove hosts for this workspace
        let keys_to_remove: Vec<_> = self
            .hosts
            .iter()
            .filter(|entry| entry.key().0 == workspace_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for key in &keys_to_remove {
            self.hosts.remove(key);
        }

        // Remove file mappings for this workspace
        let file_keys_to_remove: Vec<_> = self
            .file_to_project
            .iter()
            .filter(|entry| entry.value().0 == workspace_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for key in file_keys_to_remove {
            self.file_to_project.remove(&key);
        }

        // Remove config
        self.configs.remove(workspace_uri);
    }

    /// Update document version tracking
    ///
    /// Returns `true` if this is a valid (newer) version, `false` if stale.
    #[allow(dead_code)]
    pub fn update_document_version(&self, uri: &str, version: i32) -> bool {
        if let Some(current_version) = self.document_versions.get(uri) {
            if version <= *current_version {
                return false;
            }
        }
        self.document_versions.insert(uri.to_string(), version);
        true
    }

    /// Remove document version tracking for a closed document
    #[allow(dead_code)]
    pub fn remove_document_version(&self, uri: &str) {
        self.document_versions.remove(uri);
    }

    /// Get workspace count
    #[allow(dead_code)]
    pub fn workspace_count(&self) -> usize {
        self.workspace_roots.len()
    }

    /// Check if any workspaces are loaded
    #[allow(dead_code)]
    pub fn has_workspaces(&self) -> bool {
        !self.workspace_roots.is_empty()
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_manager_creation() {
        let manager = WorkspaceManager::new();
        assert!(manager.workspace_roots.is_empty());
        assert!(manager.hosts.is_empty());
    }

    #[test]
    fn test_get_or_create_host() {
        let manager = WorkspaceManager::new();
        let host1 = manager.get_or_create_host("workspace1", "project1");
        let host2 = manager.get_or_create_host("workspace1", "project1");

        // Should return the same host
        assert!(Arc::ptr_eq(&host1, &host2));

        // Different project should get different host
        let host3 = manager.get_or_create_host("workspace1", "project2");
        assert!(!Arc::ptr_eq(&host1, &host3));
    }

    #[test]
    fn test_register_and_clear_workspace() {
        let manager = WorkspaceManager::new();

        // Register files
        manager.register_file("file1.graphql", "workspace1", "project1");
        manager.register_file("file2.graphql", "workspace1", "project1");
        manager.register_file("file3.graphql", "workspace2", "project1");

        // Create hosts
        let _ = manager.get_or_create_host("workspace1", "project1");
        let _ = manager.get_or_create_host("workspace2", "project1");

        // Clear workspace1
        manager.clear_workspace("workspace1");

        // workspace1 data should be gone
        assert!(manager.file_to_project.get("file1.graphql").is_none());
        assert!(manager.file_to_project.get("file2.graphql").is_none());
        assert!(manager
            .hosts
            .get(&("workspace1".to_string(), "project1".to_string()))
            .is_none());

        // workspace2 data should remain
        assert!(manager.file_to_project.get("file3.graphql").is_some());
        assert!(manager
            .hosts
            .get(&("workspace2".to_string(), "project1".to_string()))
            .is_some());
    }

    #[test]
    fn test_document_version_tracking() {
        let manager = WorkspaceManager::new();

        // First version should succeed
        assert!(manager.update_document_version("file.graphql", 1));

        // Higher version should succeed
        assert!(manager.update_document_version("file.graphql", 2));

        // Same version should fail
        assert!(!manager.update_document_version("file.graphql", 2));

        // Lower version should fail
        assert!(!manager.update_document_version("file.graphql", 1));

        // Remove and re-add
        manager.remove_document_version("file.graphql");
        assert!(manager.update_document_version("file.graphql", 1));
    }
}
