//! Core MCP service implementation
//!
//! `McpService` wraps `graphql-ide` and provides the business logic for all MCP tools.
//! It can work with either an owned `AnalysisHost` or a shared `Analysis` snapshot.

use crate::types::{DiagnosticInfo, LintResult, ValidateDocumentParams, ValidateDocumentResult};
use anyhow::{Context, Result};
use graphql_ide::{Analysis, AnalysisHost, FileKind, FilePath};
use std::path::Path;

/// The core service that implements GraphQL analysis capabilities
///
/// This service can operate in two modes:
/// - **Owned mode**: Has its own `AnalysisHost`, suitable for standalone operation
/// - **Shared mode**: Uses a shared `Analysis` snapshot, suitable for embedded operation
pub struct McpService {
    /// The analysis host (owned mode)
    host: Option<AnalysisHost>,

    /// A shared analysis snapshot (shared mode)
    shared_analysis: Option<Analysis>,
}

impl McpService {
    /// Create a new McpService with an owned AnalysisHost
    pub fn new() -> Self {
        Self {
            host: Some(AnalysisHost::new()),
            shared_analysis: None,
        }
    }

    /// Create a McpService with a shared Analysis snapshot
    ///
    /// In this mode, the service uses the provided snapshot for all queries.
    /// This is useful when embedding in the LSP to share cached state.
    pub fn with_analysis(analysis: Analysis) -> Self {
        Self {
            host: None,
            shared_analysis: Some(analysis),
        }
    }

    /// Get an Analysis snapshot for querying
    fn analysis(&self) -> Analysis {
        if let Some(ref analysis) = self.shared_analysis {
            analysis.clone()
        } else if let Some(ref host) = self.host {
            host.snapshot()
        } else {
            panic!("McpService has neither host nor shared_analysis");
        }
    }

    /// Load a workspace from a directory
    ///
    /// This looks for `.graphqlrc.yaml` and loads schema/document files.
    /// Only works in owned mode.
    pub fn load_workspace(&mut self, workspace: &Path) -> Result<()> {
        let host = self
            .host
            .as_mut()
            .context("Cannot load workspace in shared mode")?;

        // Find and load config
        let config_path = graphql_config::find_config(workspace)
            .context("Failed to search for config")?
            .context("No GraphQL config file found in workspace")?;

        let config =
            graphql_config::load_config(&config_path).context("Failed to load GraphQL config")?;

        let base_dir = config_path
            .parent()
            .context("Failed to get config directory")?;

        // Get the default project (or first project)
        let project_config = config
            .projects()
            .next()
            .map(|(_, cfg)| cfg.clone())
            .context("No project found in config")?;

        // Load schemas
        host.load_schemas_from_config(&project_config, base_dir)?;

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
                                host.add_file(&file_path, &content, kind, 0);
                            }
                        }
                    }
                }
            }
        }

        host.rebuild_project_files();

        tracing::info!("Loaded workspace from {}", workspace.display());
        Ok(())
    }

    /// Validate a GraphQL document
    ///
    /// This validates the document against the loaded schema and returns
    /// any syntax errors and validation errors.
    pub fn validate_document(&mut self, params: ValidateDocumentParams) -> ValidateDocumentResult {
        let file_path = params
            .file_path
            .unwrap_or_else(|| "document.graphql".to_string());

        // In owned mode, add the document to the host temporarily
        if let Some(ref mut host) = self.host {
            let fp = FilePath::new(file_path.clone());
            host.add_file(&fp, &params.document, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();
        }

        let analysis = self.analysis();
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
    pub fn lint_document(&mut self, document: &str, file_path: Option<&str>) -> LintResult {
        let file_path = file_path.unwrap_or("document.graphql");

        // In owned mode, add the document to the host temporarily
        if let Some(ref mut host) = self.host {
            let fp = FilePath::new(file_path.to_string());
            host.add_file(&fp, document, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();
        }

        let analysis = self.analysis();
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
                rule: None, // Rule name not available in simple Diagnostic
                fix: None,  // Fixes will be added in future version
            })
            .collect();

        LintResult {
            issue_count,
            fixable_count: 0,
            diagnostics,
        }
    }

    /// Update the shared analysis snapshot
    ///
    /// This is used in embedded mode to refresh the analysis when the LSP
    /// has updated files.
    pub fn update_analysis(&mut self, analysis: Analysis) {
        self.shared_analysis = Some(analysis);
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

    #[test]
    fn test_validate_returns_result() {
        let mut service = McpService::new();

        // First add a schema
        if let Some(ref mut host) = service.host {
            host.add_file(
                &FilePath::new("schema.graphql".to_string()),
                "type Query { hello: String }",
                FileKind::Schema,
                0,
            );
            host.rebuild_project_files();
        }

        // Validate a simple query
        let result = service.validate_document(ValidateDocumentParams {
            document: "{ hello }".to_string(),
            file_path: None,
        });

        // Should successfully return a result
        assert!(result.valid);
        assert_eq!(result.error_count, 0);
    }

    #[test]
    fn test_validate_syntax_error() {
        let mut service = McpService::new();
        let result = service.validate_document(ValidateDocumentParams {
            document: "query { user { ".to_string(), // Missing closing braces
            file_path: None,
        });

        assert!(!result.valid);
        assert!(result.error_count > 0);
    }

    #[test]
    fn test_validate_valid_query() {
        let mut service = McpService::new();

        // First add a schema
        if let Some(ref mut host) = service.host {
            host.add_file(
                &FilePath::new("schema.graphql".to_string()),
                "type Query { hello: String }",
                FileKind::Schema,
                0,
            );
            host.rebuild_project_files();
        }

        let result = service.validate_document(ValidateDocumentParams {
            document: "query { hello }".to_string(),
            file_path: None,
        });

        assert!(result.valid);
        assert_eq!(result.error_count, 0);
    }
}
