//! Core MCP service implementation
//!
//! `McpService` wraps `graphql-ide` and provides the business logic for all MCP tools.
//! It can work with either owned `AnalysisHost` instances or shared `Analysis` snapshots.

use crate::types::{
    DiagnosticInfo, FileDiagnostics, LintResult, LoadProjectResult, ProjectDiagnosticsResult,
    ValidateDocumentParams, ValidateDocumentResult,
};
use crate::McpPreloadConfig;
use anyhow::{Context, Result};
use graphql_ide::{Analysis, AnalysisHost, FileKind, FilePath};
use std::collections::HashMap;
use std::path::Path;

/// The core service that implements GraphQL analysis capabilities
///
/// This service can operate in two modes:
/// - **Owned mode**: Has its own `AnalysisHost` per project, suitable for standalone operation
/// - **Shared mode**: Uses shared `Analysis` snapshots, suitable for embedded operation
pub struct McpService {
    /// Analysis hosts per project (owned mode)
    hosts: HashMap<String, AnalysisHost>,

    /// Shared analysis snapshots per project (shared mode)
    shared_analyses: HashMap<String, Analysis>,

    /// The loaded config (for listing projects)
    config: Option<graphql_config::GraphQLConfig>,

    /// Base directory for the loaded config
    config_base_dir: Option<std::path::PathBuf>,
}

impl McpService {
    /// Create a new McpService (owned mode)
    pub fn new() -> Self {
        Self {
            hosts: HashMap::new(),
            shared_analyses: HashMap::new(),
            config: None,
            config_base_dir: None,
        }
    }

    /// Create a McpService with a shared Analysis snapshot
    ///
    /// In this mode, the service uses the provided snapshot for all queries.
    /// This is useful when embedding in the LSP to share cached state.
    pub fn with_analysis(analysis: Analysis) -> Self {
        let mut shared_analyses = HashMap::new();
        shared_analyses.insert("default".to_string(), analysis);
        Self {
            hosts: HashMap::new(),
            shared_analyses,
            config: None,
            config_base_dir: None,
        }
    }

    /// Get an Analysis snapshot for a specific project
    fn analysis(&self, project: &str) -> Option<Analysis> {
        if let Some(analysis) = self.shared_analyses.get(project) {
            Some(analysis.clone())
        } else {
            self.hosts
                .get(project)
                .map(graphql_ide::AnalysisHost::snapshot)
        }
    }

    /// Get the first available project name
    fn first_project(&self) -> Option<&str> {
        self.hosts
            .keys()
            .next()
            .map(String::as_str)
            .or_else(|| self.shared_analyses.keys().next().map(String::as_str))
    }

    /// Get or create a host for a project (owned mode only)
    fn get_or_create_host(&mut self, project: &str) -> &mut AnalysisHost {
        self.hosts.entry(project.to_string()).or_default()
    }

    /// Load a workspace from a directory
    ///
    /// This looks for `.graphqlrc.yaml` and loads projects according to the preload config.
    /// Only works in owned mode.
    pub fn load_workspace(&mut self, workspace: &Path, preload: &McpPreloadConfig) -> Result<()> {
        // Find and load config
        let config_path = graphql_config::find_config(workspace)
            .context("Failed to search for config")?
            .context("No GraphQL config file found in workspace")?;

        let config =
            graphql_config::load_config(&config_path).context("Failed to load GraphQL config")?;

        let base_dir = config_path
            .parent()
            .context("Failed to get config directory")?;

        // Store config for later use (needed for load_project tool)
        self.config = Some(config.clone());
        self.config_base_dir = Some(base_dir.to_path_buf());

        // Determine which projects to load
        let projects_to_load: Vec<_> = match preload {
            McpPreloadConfig::None => {
                tracing::info!(
                    "No projects preloaded. Use load_project tool to load projects on demand."
                );
                return Ok(());
            }
            McpPreloadConfig::All => config
                .projects()
                .map(|(name, _)| name.to_string())
                .collect(),
            McpPreloadConfig::Selected(names) => names.clone(),
        };

        // Load selected projects
        for project_name in &projects_to_load {
            let Some(project_config) = config.get_project(project_name) else {
                tracing::warn!("Project '{}' not found in config, skipping", project_name);
                continue;
            };

            let host = self.get_or_create_host(project_name);

            // Load schemas
            if let Err(e) = host.load_schemas_from_config(project_config, base_dir) {
                tracing::warn!(
                    "Failed to load schemas for project '{}': {}",
                    project_name,
                    e
                );
                continue;
            }

            // Load documents if configured
            if let Some(ref documents_config) = project_config.documents {
                let patterns: Vec<_> = documents_config.patterns().into_iter().collect();

                for pattern in patterns {
                    let full_pattern = base_dir.join(pattern).display().to_string();

                    if let Ok(paths) = glob::glob(&full_pattern) {
                        for entry in paths.flatten() {
                            if entry.is_file() {
                                if let Ok(content) = std::fs::read_to_string(&entry) {
                                    let file_path =
                                        FilePath::new(entry.to_string_lossy().to_string());
                                    let kind = match entry.extension().and_then(|e| e.to_str()) {
                                        Some("ts" | "tsx") => FileKind::TypeScript,
                                        Some("js" | "jsx") => FileKind::JavaScript,
                                        _ => FileKind::ExecutableGraphQL,
                                    };
                                    host.add_file(&file_path, &content, kind);
                                }
                            }
                        }
                    }
                }
            }

            host.rebuild_project_files();
            tracing::info!(
                "Loaded project '{}' from {}",
                project_name,
                workspace.display()
            );
        }

        let project_count = self.hosts.len();
        tracing::info!(
            "Loaded {} project(s) from {}",
            project_count,
            workspace.display()
        );
        Ok(())
    }

    /// List available projects
    ///
    /// Returns project names and whether each is currently loaded.
    pub fn list_projects(&self) -> Vec<(String, bool)> {
        // If we have a config, use it to list projects
        if let Some(ref config) = self.config {
            return config
                .projects()
                .map(|(name, _)| {
                    let is_loaded =
                        self.hosts.contains_key(name) || self.shared_analyses.contains_key(name);
                    (name.to_string(), is_loaded)
                })
                .collect();
        }

        // Otherwise list from loaded hosts/analyses
        let mut projects: Vec<_> = self.hosts.keys().map(|name| (name.clone(), true)).collect();

        for name in self.shared_analyses.keys() {
            if !self.hosts.contains_key(name) {
                projects.push((name.clone(), true));
            }
        }

        projects
    }

    /// Get all diagnostics for all loaded projects
    ///
    /// Returns diagnostics grouped by file. Files with no diagnostics are excluded.
    pub fn project_diagnostics(&self) -> ProjectDiagnosticsResult {
        let mut all_files: Vec<FileDiagnostics> = Vec::new();

        // Collect diagnostics from all projects
        let project_names: Vec<_> = self
            .hosts
            .keys()
            .chain(self.shared_analyses.keys())
            .cloned()
            .collect();

        for project_name in &project_names {
            if let Some(analysis) = self.analysis(project_name) {
                let diagnostics = analysis.all_diagnostics();

                for (file_path, file_diagnostics) in diagnostics {
                    if !file_diagnostics.is_empty() {
                        all_files.push(FileDiagnostics {
                            file: file_path.as_str().to_string(),
                            diagnostics: file_diagnostics
                                .into_iter()
                                .map(DiagnosticInfo::from)
                                .collect(),
                        });
                    }
                }
            }
        }

        let total_count: usize = all_files.iter().map(|f| f.diagnostics.len()).sum();
        let file_count = all_files.len();

        ProjectDiagnosticsResult {
            project: if project_names.len() == 1 {
                Some(project_names.into_iter().next().unwrap())
            } else {
                None
            },
            total_count,
            file_count,
            files: all_files,
        }
    }

    /// Load a specific project by name
    ///
    /// Returns success/failure status with a message.
    pub fn load_project(&mut self, project_name: &str) -> LoadProjectResult {
        // Check if already loaded
        if self.hosts.contains_key(project_name) || self.shared_analyses.contains_key(project_name)
        {
            return LoadProjectResult {
                success: true,
                project: project_name.to_string(),
                message: "Project already loaded".to_string(),
            };
        }

        // Need config to load a project - clone to avoid borrow issues
        let Some(config) = self.config.clone() else {
            return LoadProjectResult {
                success: false,
                project: project_name.to_string(),
                message: "No workspace config loaded".to_string(),
            };
        };

        let Some(project_config) = config.get_project(project_name).cloned() else {
            return LoadProjectResult {
                success: false,
                project: project_name.to_string(),
                message: format!("Project '{project_name}' not found in config"),
            };
        };

        let Some(base_dir) = self.config_base_dir.clone() else {
            return LoadProjectResult {
                success: false,
                project: project_name.to_string(),
                message: "Config base directory not set".to_string(),
            };
        };

        // Create and load the host
        let host = self.get_or_create_host(project_name);

        // Load schemas
        if let Err(e) = host.load_schemas_from_config(&project_config, &base_dir) {
            return LoadProjectResult {
                success: false,
                project: project_name.to_string(),
                message: format!("Failed to load schemas: {e}"),
            };
        }

        // Load documents if configured
        if let Some(ref documents_config) = project_config.documents {
            let patterns: Vec<_> = documents_config.patterns().into_iter().collect();

            for pattern in patterns {
                let full_pattern = base_dir.join(pattern).display().to_string();

                if let Ok(paths) = glob::glob(&full_pattern) {
                    for entry in paths.flatten() {
                        if entry.is_file() {
                            if let Ok(content) = std::fs::read_to_string(&entry) {
                                let file_path = FilePath::new(entry.to_string_lossy().to_string());
                                let kind = match entry.extension().and_then(|e| e.to_str()) {
                                    Some("ts" | "tsx") => FileKind::TypeScript,
                                    Some("js" | "jsx") => FileKind::JavaScript,
                                    _ => FileKind::ExecutableGraphQL,
                                };
                                host.add_file(&file_path, &content, kind);
                            }
                        }
                    }
                }
            }
        }

        host.rebuild_project_files();

        tracing::info!("Loaded project '{}'", project_name);

        LoadProjectResult {
            success: true,
            project: project_name.to_string(),
            message: "Project loaded successfully".to_string(),
        }
    }

    /// Validate a GraphQL document
    ///
    /// This validates the document against the loaded schema and returns
    /// any syntax errors and validation errors.
    pub fn validate_document(&mut self, params: ValidateDocumentParams) -> ValidateDocumentResult {
        let file_path = params
            .file_path
            .unwrap_or_else(|| "document.graphql".to_string());

        // Determine which project to use (convert to owned to avoid borrow issues)
        let project_name = params
            .project
            .clone()
            .or_else(|| self.first_project().map(ToString::to_string))
            .unwrap_or_else(|| "default".to_string());

        // Add the document to the appropriate host
        if let Some(host) = self.hosts.get_mut(&project_name) {
            let fp = FilePath::new(file_path.clone());
            host.add_file(&fp, &params.document, FileKind::ExecutableGraphQL);
            host.rebuild_project_files();
        }

        let Some(analysis) = self.analysis(&project_name) else {
            return ValidateDocumentResult {
                valid: false,
                error_count: 1,
                warning_count: 0,
                diagnostics: vec![DiagnosticInfo {
                    severity: crate::types::DiagnosticSeverity::Error,
                    message: format!("Project '{}' not loaded", &project_name),
                    range: None,
                    rule: None,
                    fix: None,
                }],
            };
        };

        let fp = FilePath::new(file_path);
        let diagnostics = analysis.diagnostics(&fp);

        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == graphql_ide::DiagnosticSeverity::Error)
            .count();

        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == graphql_ide::DiagnosticSeverity::Warning)
            .count();

        ValidateDocumentResult {
            valid: error_count == 0,
            error_count,
            warning_count,
            diagnostics: diagnostics.into_iter().map(DiagnosticInfo::from).collect(),
        }
    }

    /// Lint a GraphQL document
    ///
    /// This runs lint rules on the document and returns any violations.
    /// Note: Auto-fix support will be added in a future version.
    pub fn lint_document(
        &mut self,
        document: &str,
        file_path: Option<&str>,
        project: Option<&str>,
    ) -> LintResult {
        let file_path = file_path.unwrap_or("document.graphql");

        // Determine which project to use (convert to owned to avoid borrow issues)
        let project_name = project
            .map(ToString::to_string)
            .or_else(|| self.first_project().map(ToString::to_string))
            .unwrap_or_else(|| "default".to_string());

        // Add the document to the appropriate host
        if let Some(host) = self.hosts.get_mut(&project_name) {
            let fp = FilePath::new(file_path.to_string());
            host.add_file(&fp, document, FileKind::ExecutableGraphQL);
            host.rebuild_project_files();
        }

        let Some(analysis) = self.analysis(&project_name) else {
            return LintResult {
                issue_count: 0,
                fixable_count: 0,
                diagnostics: vec![],
            };
        };

        let fp = FilePath::new(file_path.to_string());

        // Use lint_diagnostics which returns Diagnostic with line/column positions
        let lint_diagnostics = analysis.lint_diagnostics(&fp);

        let issue_count = lint_diagnostics.len();

        let diagnostics = lint_diagnostics
            .into_iter()
            .map(|d| DiagnosticInfo {
                severity: d.severity.into(),
                message: d.message,
                range: Some(d.range.into()),
                rule: None,
                fix: None,
            })
            .collect();

        LintResult {
            issue_count,
            fixable_count: 0,
            diagnostics,
        }
    }

    /// Update the shared analysis snapshot for a project
    ///
    /// This is used in embedded mode to refresh the analysis when the LSP
    /// has updated files.
    pub fn update_analysis(&mut self, project: &str, analysis: Analysis) {
        self.shared_analyses.insert(project.to_string(), analysis);
    }
}

impl Default for McpService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_service_with_schema(schema: &str) -> McpService {
        let mut service = McpService::new();
        let host = service.get_or_create_host("default");
        host.add_file(
            &FilePath::new("schema.graphql".to_string()),
            schema,
            FileKind::Schema,
        );
        host.rebuild_project_files();
        service
    }

    #[test]
    fn test_validate_returns_result() {
        let mut service = setup_service_with_schema("type Query { hello: String }");

        let result = service.validate_document(ValidateDocumentParams {
            document: "{ hello }".to_string(),
            file_path: None,
            project: None,
        });

        assert!(result.valid);
        assert_eq!(result.error_count, 0);
    }

    #[test]
    fn test_validate_syntax_error() {
        let mut service = setup_service_with_schema("type Query { hello: String }");

        let result = service.validate_document(ValidateDocumentParams {
            document: "query { user { ".to_string(),
            file_path: None,
            project: None,
        });

        assert!(!result.valid);
        assert!(result.error_count > 0);
    }

    #[test]
    fn test_validate_valid_query() {
        let mut service = setup_service_with_schema("type Query { hello: String }");

        let result = service.validate_document(ValidateDocumentParams {
            document: "query { hello }".to_string(),
            file_path: None,
            project: None,
        });

        assert!(result.valid);
        assert_eq!(result.error_count, 0);
    }

    #[test]
    fn test_multiple_projects() {
        let mut service = McpService::new();

        // Create two projects with different schemas
        let host1 = service.get_or_create_host("project1");
        host1.add_file(
            &FilePath::new("schema.graphql".to_string()),
            "type Query { users: [User] } type User { id: ID! }",
            FileKind::Schema,
        );
        host1.rebuild_project_files();

        let host2 = service.get_or_create_host("project2");
        host2.add_file(
            &FilePath::new("schema.graphql".to_string()),
            "type Query { posts: [Post] } type Post { title: String }",
            FileKind::Schema,
        );
        host2.rebuild_project_files();

        // Validate against project1
        let result1 = service.validate_document(ValidateDocumentParams {
            document: "{ users { id } }".to_string(),
            file_path: None,
            project: Some("project1".to_string()),
        });
        assert!(result1.valid);

        // Same query should fail against project2 (no users field)
        let result2 = service.validate_document(ValidateDocumentParams {
            document: "{ users { id } }".to_string(),
            file_path: None,
            project: Some("project2".to_string()),
        });
        assert!(!result2.valid);

        // Validate against project2
        let result3 = service.validate_document(ValidateDocumentParams {
            document: "{ posts { title } }".to_string(),
            file_path: None,
            project: Some("project2".to_string()),
        });
        assert!(result3.valid);
    }

    #[test]
    fn test_list_projects() {
        let mut service = McpService::new();

        // Initially empty
        assert!(service.list_projects().is_empty());

        // Add projects
        service.get_or_create_host("project1");
        service.get_or_create_host("project2");

        let projects = service.list_projects();
        assert_eq!(projects.len(), 2);
        assert!(projects.iter().any(|(name, _)| name == "project1"));
        assert!(projects.iter().any(|(name, _)| name == "project2"));
    }
}
