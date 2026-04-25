//! Workspace management for the GraphQL Language Server.
//!
//! This module provides the `WorkspaceManager` struct which manages:
//! - Workspace folder tracking
//! - Configuration paths and loaded configs
//! - `AnalysisHost` instances per workspace/project (directly owned, no locks)
//! - File-to-project mapping for efficient lookups
//!
//! ## Architecture
//!
//! All state is owned by the main thread via `GlobalState`. No locks are needed
//! because the main thread is the sole writer, and worker threads only receive
//! immutable `Analysis` snapshots.

use std::collections::HashMap;
use std::path::PathBuf;

use graphql_ide::AnalysisHost;
use lsp_types::Uri;

#[cfg(feature = "native")]
use crate::conversions::uri_to_file_path;

/// Manages workspace state for the GraphQL Language Server.
pub struct WorkspaceManager {
    /// Workspace folders from initialization (drained during initialized handler)
    pub init_workspace_folders: HashMap<String, PathBuf>,

    /// Workspace roots indexed by workspace folder URI string
    pub workspace_roots: HashMap<String, PathBuf>,

    /// Config file paths indexed by workspace URI string
    pub config_paths: HashMap<String, PathBuf>,

    /// Loaded GraphQL configs indexed by workspace URI string
    pub configs: HashMap<String, graphql_config::GraphQLConfig>,

    /// `AnalysisHost` per (workspace URI, project name) tuple.
    hosts: HashMap<(String, String), AnalysisHost>,

    /// Document versions indexed by document URI string
    pub document_versions: HashMap<String, i32>,

    /// In-memory document contents indexed by document URI string.
    pub document_contents: HashMap<String, String>,

    /// Reverse index: file URI -> (`workspace_uri`, `project_name`)
    pub file_to_project: HashMap<String, (String, String)>,

    /// Resolved schema paths per (`workspace_uri`, `project_name`).
    pub resolved_schema_paths: HashMap<(String, String), PathBuf>,
}

impl WorkspaceManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            init_workspace_folders: HashMap::new(),
            workspace_roots: HashMap::new(),
            config_paths: HashMap::new(),
            configs: HashMap::new(),
            hosts: HashMap::new(),
            document_versions: HashMap::new(),
            document_contents: HashMap::new(),
            file_to_project: HashMap::new(),
            resolved_schema_paths: HashMap::new(),
        }
    }

    /// Get or create an `AnalysisHost` for a workspace/project
    pub fn get_or_create_host(
        &mut self,
        workspace_uri: &str,
        project_name: &str,
    ) -> &mut AnalysisHost {
        self.hosts
            .entry((workspace_uri.to_string(), project_name.to_string()))
            .or_default()
    }

    /// Get an existing `AnalysisHost` reference
    pub fn get_host(&self, workspace_uri: &str, project_name: &str) -> Option<&AnalysisHost> {
        self.hosts
            .get(&(workspace_uri.to_string(), project_name.to_string()))
    }

    /// Get a mutable reference to an existing host
    pub fn get_host_mut(
        &mut self,
        workspace_uri: &str,
        project_name: &str,
    ) -> Option<&mut AnalysisHost> {
        self.hosts
            .get_mut(&(workspace_uri.to_string(), project_name.to_string()))
    }

    /// Return all (key, host) pairs
    pub fn all_hosts(&self) -> impl Iterator<Item = (&(String, String), &AnalysisHost)> {
        self.hosts.iter()
    }

    /// Return hosts for a given workspace
    pub fn projects_for_workspace(&self, workspace_uri: &str) -> Vec<(&str, &AnalysisHost)> {
        self.hosts
            .iter()
            .filter(|((ws, _), _)| ws == workspace_uri)
            .map(|((_, name), host)| (name.as_str(), host))
            .collect()
    }

    /// Find the workspace and project for a given document URI
    pub fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
        let uri_string = document_uri.to_string();

        if let Some(entry) = self.file_to_project.get(&uri_string) {
            return Some(entry.clone());
        }

        // For virtual files (non-file:// scheme), search all hosts
        if !uri_string.starts_with("file://") {
            return self.find_host_for_virtual_file(&uri_string);
        }

        #[cfg(feature = "native")]
        {
            let doc_path = uri_to_file_path(document_uri)?;
            for (workspace_uri, workspace_path) in &self.workspace_roots {
                if doc_path.starts_with(workspace_path.as_path()) {
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
        }

        None
    }

    /// Find which host contains a virtual file by searching all hosts.
    fn find_host_for_virtual_file(&self, uri_string: &str) -> Option<(String, String)> {
        let file_path = graphql_ide::FilePath::new(uri_string);

        for ((workspace_uri, project_name), host) in &self.hosts {
            let snapshot = host.snapshot();
            if snapshot.file_content(&file_path).is_some() {
                return Some((workspace_uri.clone(), project_name.clone()));
            }
        }

        None
    }

    /// Clear all state for a workspace
    pub fn clear_workspace(&mut self, workspace_uri: &str) {
        self.hosts.retain(|(ws, _), _| ws != workspace_uri);
        self.file_to_project
            .retain(|_, (ws, _)| ws != workspace_uri);
        self.configs.remove(workspace_uri);
    }

    /// Get the file type (schema or document) for a file based on config patterns.
    pub fn get_file_type(
        &self,
        uri: &Uri,
        workspace_uri: &str,
        project_name: &str,
    ) -> Option<graphql_config::FileType> {
        #[cfg(not(feature = "native"))]
        {
            let _ = (uri, workspace_uri, project_name);
            return None;
        }

        #[cfg(feature = "native")]
        {
            let doc_path = uri_to_file_path(uri)?;
            let workspace_path = self.workspace_roots.get(workspace_uri)?;
            let config = self.configs.get(workspace_uri)?;
            config.get_file_type(&doc_path, workspace_path, project_name)
        }
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
        let mut manager = WorkspaceManager::new();
        let _host1 = manager.get_or_create_host("workspace1", "project1");
        let _host2 = manager.get_or_create_host("workspace1", "project2");
        assert!(manager.get_host("workspace1", "project1").is_some());
        assert!(manager.get_host("workspace1", "project2").is_some());
    }

    #[test]
    fn test_register_and_clear_workspace() {
        let mut manager = WorkspaceManager::new();

        manager.file_to_project.insert(
            "file1.graphql".to_string(),
            ("workspace1".to_string(), "project1".to_string()),
        );
        manager.file_to_project.insert(
            "file2.graphql".to_string(),
            ("workspace1".to_string(), "project1".to_string()),
        );
        manager.file_to_project.insert(
            "file3.graphql".to_string(),
            ("workspace2".to_string(), "project1".to_string()),
        );

        let _ = manager.get_or_create_host("workspace1", "project1");
        let _ = manager.get_or_create_host("workspace2", "project1");

        manager.clear_workspace("workspace1");

        assert!(!manager.file_to_project.contains_key("file1.graphql"));
        assert!(!manager.file_to_project.contains_key("file2.graphql"));
        assert!(manager.get_host("workspace1", "project1").is_none());

        assert!(manager.file_to_project.contains_key("file3.graphql"));
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
        let mut content = String::new();

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
}
