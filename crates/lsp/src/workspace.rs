//! Workspace management for the GraphQL Language Server.
//!
//! This module provides the `WorkspaceManager` struct which manages:
//! - Workspace folder tracking
//! - Configuration paths and loaded configs
//! - `AnalysisHost` instances per workspace/project
//! - File-to-project mapping for efficient lookups
//!
//! ## Architecture
//!
//! The workspace manager separates concerns:
//! - **Server**: Handles LSP protocol messages
//! - **`WorkspaceManager`**: Manages workspace state and project data
//! - **`AnalysisHost`** (in graphql-ide): Handles IDE features for a single project

use dashmap::DashMap;
use graphql_ide::AnalysisHost;
use lsp_types::Uri;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tower_lsp_server::ls_types as lsp_types;

use crate::server::describe_join_error;

/// Default timeout for acquiring host locks during LSP requests.
const LOCK_TIMEOUT: Duration = Duration::from_millis(500);

/// A wrapper around `AnalysisHost` that enforces safe access patterns.
///
/// Exposes only:
/// - [`try_snapshot`](Self::try_snapshot): timed read access for request
///   handlers, returns `None` if the host is busy.
/// - [`with_write`](Self::with_write): untimed write access for
///   initialization and config reload paths.
/// - [`add_file_and_snapshot`](Self::add_file_and_snapshot): the editing
///   hot path. Runs on `spawn_blocking` because the underlying Salsa setter
///   waits for in-flight snapshots to drop.
///
/// ## Salsa Snapshot Safety
///
/// Salsa setters block until all outstanding snapshot clones are dropped. If
/// a setter ran on the async runtime while a `spawn_blocking` task held a
/// snapshot, the runtime thread would be blocked and could not drive the
/// task that owns the snapshot — deadlock. `add_file_and_snapshot` therefore
/// uses `lock_owned()` + `spawn_blocking` so the setter parks a pool thread
/// instead of the runtime.
///
/// In-flight snapshots can no longer take a `parking_lot` lock against the
/// host (snapshots resolve everything through Salsa inputs), so the only
/// remaining concern is the runtime starvation above — the lock-ordering
/// deadlock class is gone.
#[derive(Clone)]
pub struct ProjectHost {
    inner: Arc<Mutex<AnalysisHost>>,
}

impl ProjectHost {
    /// Create a new `ProjectHost` wrapping a fresh `AnalysisHost`
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AnalysisHost::new())),
        }
    }

    /// Check if two `ProjectHost` instances point to the same underlying host
    #[cfg(test)]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Try to get a snapshot with a timeout.
    ///
    /// The only way request handlers should access analysis. Returns `None`
    /// if the lock can't be acquired within the timeout, allowing the handler
    /// to return early instead of blocking the runtime thread.
    pub async fn try_snapshot(&self) -> Option<graphql_ide::Analysis> {
        let Ok(guard) = tokio::time::timeout(LOCK_TIMEOUT, self.inner.lock()).await else {
            tracing::warn!("try_snapshot: timed out waiting for ProjectHost lock");
            return None;
        };
        Some(guard.snapshot())
    }

    /// Execute a write operation on the host.
    ///
    /// Acquires the lock without a timeout. Use only for:
    /// - Background initialization tasks
    /// - Config reload handlers
    ///
    /// Runs the closure on the async thread. Safe whenever the work is short
    /// and not on the editing hot path; for `did_change` use
    /// [`add_file_and_snapshot`](Self::add_file_and_snapshot) instead.
    pub async fn with_write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut AnalysisHost) -> R,
    {
        let mut guard = self.inner.lock().await;
        f(&mut guard)
    }

    /// Add or update a file and get a snapshot in one lock acquisition.
    ///
    /// New files automatically rebuild the project file index before
    /// snapshotting; existing files just bump the file's `FileContent`.
    ///
    /// Runs on `spawn_blocking` so the Salsa setter — which may park waiting
    /// for outstanding snapshots from previous diagnostics computations to be
    /// dropped — blocks a pool thread rather than the async runtime.
    pub async fn add_file_and_snapshot(
        &self,
        path: &graphql_ide::FilePath,
        content: &str,
        language: graphql_ide::Language,
        document_kind: graphql_ide::DocumentKind,
    ) -> (bool, graphql_ide::Analysis) {
        let mut guard = Arc::clone(&self.inner).lock_owned().await;
        let path_str = path.as_str().to_string();
        let path_owned = path.clone();
        let content = content.to_string();
        match tokio::task::spawn_blocking(move || {
            guard.update_file_and_snapshot(&path_owned, &content, language, document_kind)
        })
        .await
        {
            Ok(result) => result,
            Err(join_err) => {
                let payload = describe_join_error(join_err);
                tracing::error!(
                    path = %path_str,
                    "add_file_and_snapshot: blocking task ended abnormally: {payload}",
                );
                // Re-raise so the caller's request fails loudly rather than the
                // server silently degrading. The OwnedMutexGuard moved into the
                // closure has already been dropped during unwinding, so the
                // host's tokio Mutex is released.
                panic!("add_file_and_snapshot: blocking task panicked: {payload}");
            }
        }
    }
}

impl Default for ProjectHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages workspace state for the GraphQL Language Server.
///
/// This struct holds all per-workspace and per-project data:
/// - Workspace folder paths
/// - Configuration file paths and loaded configs
/// - `AnalysisHost` instances (one per project)
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

    /// `ProjectHost` per (workspace URI, project name) tuple.
    ///
    /// **Private by design.** All access goes through typed methods (`get_or_create_host`,
    /// `get_host`, `projects_for_workspace`, `clear_workspace`) that return owned
    /// `ProjectHost` clones. Never expose a `DashMap` `Ref` to callers — holding one
    /// across an `.await` point deadlocks the async runtime.
    hosts: DashMap<(String, String), ProjectHost>,

    /// Document versions indexed by document URI string
    /// Used to detect out-of-order updates and avoid race conditions
    pub document_versions: DashMap<String, i32>,

    /// In-memory document contents indexed by document URI string.
    /// Required for incremental text sync: the client sends only changed ranges,
    /// so we must maintain the full document text to apply edits.
    pub document_contents: DashMap<String, String>,

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
            document_contents: DashMap::new(),
            file_to_project: DashMap::new(),
        }
    }

    /// Get or create a `ProjectHost` for a workspace/project
    pub fn get_or_create_host(&self, workspace_uri: &str, project_name: &str) -> ProjectHost {
        self.hosts
            .entry((workspace_uri.to_string(), project_name.to_string()))
            .or_default()
            .clone()
    }

    /// Get an existing `ProjectHost`, returning `None` if it doesn't exist.
    ///
    /// Always returns a cloned `ProjectHost` (cheap `Arc` clone) rather than a `DashMap` reference.
    /// This is intentional: `DashMap::get()` returns a `Ref` that holds a shard lock, and holding
    /// that lock across `.await` points causes deadlocks with any concurrent `entry()` call on the
    /// same shard. Callers receive ownership and can safely `.await` without holding any `DashMap` lock.
    pub fn get_host(&self, workspace_uri: &str, project_name: &str) -> Option<ProjectHost> {
        self.hosts
            .get(&(workspace_uri.to_string(), project_name.to_string()))
            .map(|r| r.clone())
    }

    /// Return all `(project_name, ProjectHost)` pairs for a given workspace.
    ///
    /// Collects into an owned `Vec` so no `DashMap` shard lock is held after this call returns.
    pub fn projects_for_workspace(&self, workspace_uri: &str) -> Vec<(String, ProjectHost)> {
        self.hosts
            .iter()
            .filter(|entry| entry.key().0 == workspace_uri)
            .map(|entry| (entry.key().1.clone(), entry.value().clone()))
            .collect()
    }

    /// Return all hosts across all workspaces as owned `ProjectHost` clones.
    ///
    /// Collects into an owned `Vec` so no `DashMap` shard lock is held after this call returns.
    /// Use this instead of iterating `hosts` directly when you need to `.await` on each host.
    pub fn all_hosts(&self) -> Vec<ProjectHost> {
        self.hosts
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Find the workspace and project for a given document URI (sync version)
    ///
    /// Uses a reverse index for O(1) lookup of previously seen files.
    /// Falls back to config pattern matching for files opened after init.
    ///
    /// Note: For virtual files (non-file:// scheme), use `find_workspace_and_project_async`.
    pub fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
        let uri_string = document_uri.to_string();

        // First, check the reverse index
        if let Some(entry) = self.file_to_project.get(&uri_string) {
            return Some(entry.value().clone());
        }

        // For virtual files, caller should use async version
        if !uri_string.starts_with("file://") {
            return None;
        }

        // Fall back to searching configs for pattern matching
        let doc_path = document_uri.to_file_path()?;
        for workspace_entry in &self.workspace_roots {
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

    /// Find the workspace and project for a given document URI (async version)
    ///
    /// This version also handles virtual files (like `schema://` URIs) by
    /// searching all hosts asynchronously.
    #[allow(dead_code)]
    pub async fn find_workspace_and_project_async(
        &self,
        document_uri: &Uri,
    ) -> Option<(String, String)> {
        let uri_string = document_uri.to_string();

        // First, check the reverse index
        if let Some(entry) = self.file_to_project.get(&uri_string) {
            return Some(entry.value().clone());
        }

        // For virtual files (non-file:// scheme), search all hosts
        if !uri_string.starts_with("file://") {
            return self.find_host_for_virtual_file(&uri_string).await;
        }

        // Fall back to searching configs for pattern matching
        let doc_path = document_uri.to_file_path()?;
        for workspace_entry in &self.workspace_roots {
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

    /// Find which host contains a virtual file by searching all hosts.
    ///
    /// This is used for non-file:// URIs like `schema://` virtual files
    /// that represent remote schemas fetched via introspection.
    ///
    /// Note: This is async because it uses the timeout-based snapshot access.
    #[allow(dead_code)]
    pub async fn find_host_for_virtual_file(&self, uri_string: &str) -> Option<(String, String)> {
        let file_path = graphql_ide::FilePath::new(uri_string);

        for entry in &self.hosts {
            let (workspace_uri, project_name) = entry.key();
            let host = entry.value();

            // Try to get a snapshot with timeout
            if let Some(snapshot) = host.try_snapshot().await {
                if snapshot.file_content(&file_path).is_some() {
                    return Some((workspace_uri.clone(), project_name.clone()));
                }
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

    /// Get the file type (schema or document) for a file based on config patterns.
    ///
    /// This determines whether a file should be treated as a schema file or
    /// a document file based on the project's configuration patterns.
    pub fn get_file_type(
        &self,
        uri: &Uri,
        workspace_uri: &str,
        project_name: &str,
    ) -> Option<graphql_config::FileType> {
        let doc_path = uri.to_file_path()?;
        let workspace_path = self.workspace_roots.get(workspace_uri)?;
        let config = self.configs.get(workspace_uri)?;
        config.get_file_type(&doc_path, workspace_path.value(), project_name)
    }
}

/// Apply an incremental content change to document text.
///
/// Handles both full-document replacements (range is None) and incremental
/// changes (range specifies the region to replace). Uses UTF-16 code unit
/// positions as specified by the LSP protocol.
pub fn apply_content_change(
    content: &str,
    change: &lsp_types::TextDocumentContentChangeEvent,
) -> String {
    let Some(range) = change.range else {
        // Full document replacement
        return change.text.clone();
    };

    let line_index = graphql_syntax::LineIndex::new(content);

    let start_offset = line_index.utf16_to_offset(range.start.line as usize, range.start.character);
    let end_offset = line_index.utf16_to_offset(range.end.line as usize, range.end.character);

    if let (Some(start), Some(end)) = (start_offset, end_offset) {
        let mut result = String::with_capacity(content.len() - (end - start) + change.text.len());
        result.push_str(&content[..start]);
        result.push_str(&change.text);
        result.push_str(&content[end..]);
        result
    } else {
        tracing::warn!(
            "Failed to resolve incremental change offsets, falling back to full replacement"
        );
        change.text.clone()
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
        assert!(manager.get_host("nonexistent", "nonexistent").is_none());
    }

    #[test]
    fn test_get_or_create_host() {
        let manager = WorkspaceManager::new();
        let host1 = manager.get_or_create_host("workspace1", "project1");
        let host2 = manager.get_or_create_host("workspace1", "project1");

        // Should return the same host
        assert!(host1.ptr_eq(&host2));

        // Different project should get different host
        let host3 = manager.get_or_create_host("workspace1", "project2");
        assert!(!host1.ptr_eq(&host3));
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
        assert!(manager.get_host("workspace1", "project1").is_none());

        // workspace2 data should remain
        assert!(manager.file_to_project.get("file3.graphql").is_some());
        assert!(manager.get_host("workspace2", "project1").is_some());
    }

    #[test]
    fn test_apply_content_change_full_replacement() {
        let content = "query { hello }";
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "query { world }".to_string(),
        };
        assert_eq!(apply_content_change(content, &change), "query { world }");
    }

    #[test]
    fn test_apply_content_change_single_char_insert() {
        // Insert "x" at position (0, 8) in "query { hello }"
        let content = "query { hello }";
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
            }),
            range_length: None,
            text: "x".to_string(),
        };
        assert_eq!(apply_content_change(content, &change), "query { xhello }");
    }

    #[test]
    fn test_apply_content_change_replace_word() {
        // Replace "hello" (positions 8..13) with "world"
        let content = "query { hello }";
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 13,
                },
            }),
            range_length: None,
            text: "world".to_string(),
        };
        assert_eq!(apply_content_change(content, &change), "query { world }");
    }

    #[test]
    fn test_apply_content_change_multiline() {
        let content = "query {\n  hello\n  world\n}";
        // Replace "hello" on line 1, chars 2..7
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 1,
                    character: 2,
                },
                end: lsp_types::Position {
                    line: 1,
                    character: 7,
                },
            }),
            range_length: None,
            text: "foo".to_string(),
        };
        assert_eq!(
            apply_content_change(content, &change),
            "query {\n  foo\n  world\n}"
        );
    }

    #[test]
    fn test_apply_content_change_delete() {
        // Delete "hello " (positions 8..14)
        let content = "query { hello world }";
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 14,
                },
            }),
            range_length: None,
            text: String::new(),
        };
        assert_eq!(apply_content_change(content, &change), "query { world }");
    }

    #[test]
    fn test_apply_content_change_cross_line() {
        let content = "query {\n  hello\n  world\n}";
        // Replace from end of line 1 to start of line 2's content
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 1,
                    character: 2,
                },
                end: lsp_types::Position {
                    line: 2,
                    character: 7,
                },
            }),
            range_length: None,
            text: "combined".to_string(),
        };
        assert_eq!(
            apply_content_change(content, &change),
            "query {\n  combined\n}"
        );
    }

    #[test]
    fn test_apply_content_change_sequential_edits() {
        // Simulate a realistic editing session: type "query { }" character by character
        let mut content = String::new();

        // Type "q"
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
            }),
            range_length: None,
            text: "q".to_string(),
        };
        content = apply_content_change(&content, &change);
        assert_eq!(content, "q");

        // Type "uery { }"
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 1,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 1,
                },
            }),
            range_length: None,
            text: "uery { }".to_string(),
        };
        content = apply_content_change(&content, &change);
        assert_eq!(content, "query { }");

        // Insert "name " between "{ " and "}"
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
            }),
            range_length: None,
            text: "name ".to_string(),
        };
        content = apply_content_change(&content, &change);
        assert_eq!(content, "query { name }");
    }

    #[test]
    fn test_document_contents_tracking() {
        let manager = WorkspaceManager::new();
        let uri = "file:///test.graphql";

        // Store initial content (simulating did_open)
        manager
            .document_contents
            .insert(uri.to_string(), "query { hello }".to_string());

        // Apply incremental change (simulating did_change)
        let stored = manager.document_contents.get(uri).unwrap();
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 13,
                },
            }),
            range_length: None,
            text: "world".to_string(),
        };
        let new_content = apply_content_change(&stored, &change);
        drop(stored);
        manager
            .document_contents
            .insert(uri.to_string(), new_content);

        assert_eq!(
            *manager.document_contents.get(uri).unwrap(),
            "query { world }"
        );

        // Clean up (simulating did_close)
        manager.document_contents.remove(uri);
        assert!(manager.document_contents.get(uri).is_none());
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

    /// Regression test for deadlock when rapidly editing schema files.
    ///
    /// Reproduces the scenario where `add_file_and_snapshot` blocks the async
    /// runtime because a previous Salsa snapshot is still held by a
    /// `spawn_blocking` task. The Salsa setter waits for all snapshots to be
    /// dropped before it can proceed, and if the setter runs on the async
    /// runtime thread, it starves.
    ///
    /// Uses a single-threaded runtime to make the bug deterministic: if the
    /// Salsa setter blocks the one runtime thread, no other async work can
    /// proceed. The test uses an OS-level thread timeout to detect the hang
    /// (tokio's own timers can't fire when the runtime thread is blocked).
    #[test]
    fn test_add_file_and_snapshot_does_not_block_async_runtime() {
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let host = ProjectHost::new();
                let path = graphql_ide::FilePath::new("file:///test.graphql");

                // Step 1: Add a file and get a snapshot
                let (_is_new, snapshot) = host
                    .add_file_and_snapshot(
                        &path,
                        "type Query { hello: String }",
                        graphql_ide::Language::GraphQL,
                        graphql_ide::DocumentKind::Schema,
                    )
                    .await;

                // Step 2: Hold the snapshot on a blocking thread (simulates
                // spawn_blocking diagnostics computation)
                let (held_tx, held_rx) = tokio::sync::oneshot::channel::<()>();
                let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

                tokio::task::spawn_blocking(move || {
                    let _ = held_tx.send(());
                    let _ = release_rx.blocking_recv();
                    drop(snapshot);
                });

                held_rx.await.unwrap();

                // Step 3: Start the mutation as a spawned task so we can
                // observe whether the runtime stays responsive.
                let host_clone = host.clone();
                let mutation_handle = tokio::spawn(async move {
                    host_clone
                        .add_file_and_snapshot(
                            &graphql_ide::FilePath::new("file:///test.graphql"),
                            "type Query { hello: String! }",
                            graphql_ide::Language::GraphQL,
                            graphql_ide::DocumentKind::Schema,
                        )
                        .await
                });

                // Yield to let the mutation task start executing
                tokio::task::yield_now().await;

                // Step 4: Check if the runtime is still responsive. If the
                // mutation blocked the runtime thread (bug), this sleep can
                // never complete because the thread is stuck in the Salsa
                // setter. If the mutation runs in spawn_blocking (fix), the
                // runtime thread is free and this completes normally.
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;

                // Clean up
                let _ = release_tx.send(());
                let _ = mutation_handle.await;
            });

            let _ = done_tx.send(());
        });

        // OS-level timeout: tokio timers can't fire when the runtime is
        // blocked, so we detect the hang from an external thread.
        let result = done_rx.recv_timeout(Duration::from_secs(5));
        assert!(
            result.is_ok(),
            "add_file_and_snapshot blocked the async runtime waiting for \
             a Salsa snapshot to be dropped — the Salsa setter must run \
             in spawn_blocking, not on the async thread"
        );
    }

    /// Regression test for the lock-ordering deadlock that motivated the
    /// `FilePathMap` refactor.
    ///
    /// **The bug**: snapshots used to hold a `parking_lot::RwLock` read on the
    /// host's `FileRegistry` to resolve file paths. The `did_change` writer
    /// took the same `RwLock` for write inside `update_file_and_snapshot`,
    /// then called a Salsa setter that waits for outstanding snapshots to
    /// drop. With a long-running snapshot taking repeated `registry.read()`
    /// calls, `parking_lot`'s writer-preferring policy starved those reads,
    /// the snapshot couldn't drop, and the setter parked forever. Two
    /// `spawn_blocking` worker threads, each waiting on the other.
    ///
    /// **The fix verified here**: snapshots resolve everything via Salsa
    /// (`FilePathMap`, `FileEntryMap`), so they hold no `parking_lot` lock
    /// and the cycle is structurally impossible.
    ///
    /// The test seeds the host with a schema and 50 documents, then runs a
    /// long-lived snapshot in `spawn_blocking` that loops over file lookups
    /// (`diagnostics`, `file_content`, `workspace_symbols`) while a
    /// concurrent task drives `add_file_and_snapshot` 20 times with new file
    /// URIs (forcing the new-file path that previously held the registry
    /// write lock across the Salsa setter). On a single-threaded runtime
    /// with an OS-level 10s timeout, this hangs pre-refactor and finishes
    /// quickly post-refactor.
    #[test]
    fn test_concurrent_snapshot_lookups_during_writer() {
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let host = ProjectHost::new();

                // Seed: schema + 50 documents.
                let schema_path = graphql_ide::FilePath::new("file:///schema.graphql");
                let _ = host
                    .add_file_and_snapshot(
                        &schema_path,
                        "type Query { hello: String, items: [Item] } \
                         type Item { id: ID!, name: String }",
                        graphql_ide::Language::GraphQL,
                        graphql_ide::DocumentKind::Schema,
                    )
                    .await;
                for i in 0..50 {
                    let doc_path = graphql_ide::FilePath::new(format!("file:///doc-{i}.graphql"));
                    let _ = host
                        .add_file_and_snapshot(
                            &doc_path,
                            "query Q { hello }",
                            graphql_ide::Language::GraphQL,
                            graphql_ide::DocumentKind::Executable,
                        )
                        .await;
                }

                // Take a fresh snapshot and hand it to a blocking worker that
                // exercises the same lookups (`diagnostics`, `file_content`,
                // `workspace_symbols`) the LSP would on the read path. Each
                // call goes through the Salsa-backed file lookups; pre-refactor
                // these would have taken `registry.read()` and parked once a
                // writer was queued.
                let snapshot = host.try_snapshot().await.expect("snapshot");
                let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
                let reader = tokio::task::spawn_blocking(move || {
                    for _ in 0..200 {
                        for i in 0..50 {
                            let p = graphql_ide::FilePath::new(format!("file:///doc-{i}.graphql"));
                            let _ = snapshot.diagnostics(&p);
                            let _ = snapshot.file_content(&p);
                        }
                        let _ = snapshot.workspace_symbols("Q");
                    }
                    // Hold the snapshot until the writer loop signals done so
                    // we exercise the "snapshot in flight while writer runs"
                    // condition that triggers the deadlock pre-refactor.
                    let _ = release_rx.blocking_recv();
                    drop(snapshot);
                });

                // Concurrently, drive 20 new-file additions. Each one forces
                // a `rebuild_project_files` and the Salsa setter that
                // previously parked on snapshot drop.
                for i in 0..20 {
                    let new_path = graphql_ide::FilePath::new(format!("file:///new-{i}.graphql"));
                    let _ = host
                        .add_file_and_snapshot(
                            &new_path,
                            "query NewQ { hello }",
                            graphql_ide::Language::GraphQL,
                            graphql_ide::DocumentKind::Executable,
                        )
                        .await;
                }

                let _ = release_tx.send(());
                let _ = reader.await;
            });

            let _ = done_tx.send(());
        });

        let result = done_rx.recv_timeout(Duration::from_secs(10));
        assert!(
            result.is_ok(),
            "Concurrent snapshot lookups deadlocked the writer — snapshots \
             must resolve all file lookups through Salsa, never through a \
             side-channel parking_lot lock shared with the host."
        );
    }
}
