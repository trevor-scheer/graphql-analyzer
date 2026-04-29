//! Core MCP service implementation
//!
//! `McpService` wraps `graphql-ide` and provides the business logic for all MCP tools.
//! It can work with either owned `AnalysisHost` instances or shared `Analysis` snapshots.

use crate::types::{
    ArgumentInfo, CompletionInfo, CompletionsResult, ComplexityInfo, DiagnosticInfo,
    DirectiveArgumentInfo, DirectiveInfo, DocumentSymbolsResult, EnumValueInfo,
    FieldComplexityInfo, FieldInfo, FileDiagnostics, HoverResultInfo, LintResult,
    LoadProjectResult, LocationResult, LocationsResult, OperationInfo, OperationsResult,
    ProjectDiagnosticsResult, QueryComplexityResult, SchemaSdlResult, SchemaStatsInfo,
    SchemaTypeInfo, SchemaTypesResult, SymbolInfo, TypeInfoResult, ValidateDocumentParams,
    ValidateDocumentResult, VariableInfo, WorkspaceSymbolInfo, WorkspaceSymbolsResult,
};
use crate::McpPreloadConfig;
use anyhow::{Context, Result};
use graphql_ide::{Analysis, AnalysisHost, DocumentKind, FilePath, Language};
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
                let mut files_to_add: Vec<(FilePath, String, Language, DocumentKind)> = Vec::new();

                for pattern in patterns {
                    let full_pattern = base_dir.join(pattern).display().to_string();

                    if let Ok(paths) = glob::glob(&full_pattern) {
                        for entry in paths.flatten() {
                            if entry.is_file() {
                                if let Ok(content) = std::fs::read_to_string(&entry) {
                                    let file_path = FilePath::from_path(&entry);
                                    let (language, document_kind) =
                                        match entry.extension().and_then(|e| e.to_str()) {
                                            Some("ts" | "tsx") => {
                                                (Language::TypeScript, DocumentKind::Executable)
                                            }
                                            Some("js" | "jsx") => {
                                                (Language::JavaScript, DocumentKind::Executable)
                                            }
                                            _ => (Language::GraphQL, DocumentKind::Executable),
                                        };
                                    files_to_add.push((
                                        file_path,
                                        content,
                                        language,
                                        document_kind,
                                    ));
                                }
                            }
                        }
                    }
                }

                // Batch add all files for O(n) performance
                let batch_refs: Vec<(FilePath, &str, Language, DocumentKind)> = files_to_add
                    .iter()
                    .map(|(path, content, language, document_kind)| {
                        (path.clone(), content.as_str(), *language, *document_kind)
                    })
                    .collect();
                host.add_files_batch(&batch_refs);
            } else {
                // No documents to load, but still need to rebuild for schemas
                host.rebuild_project_files();
            }
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
            let mut files_to_add: Vec<(FilePath, String, Language, DocumentKind)> = Vec::new();

            for pattern in patterns {
                let full_pattern = base_dir.join(pattern).display().to_string();

                if let Ok(paths) = glob::glob(&full_pattern) {
                    for entry in paths.flatten() {
                        if entry.is_file() {
                            if let Ok(content) = std::fs::read_to_string(&entry) {
                                let file_path = FilePath::from_path(&entry);
                                let (language, document_kind) =
                                    match entry.extension().and_then(|e| e.to_str()) {
                                        Some("ts" | "tsx") => {
                                            (Language::TypeScript, DocumentKind::Executable)
                                        }
                                        Some("js" | "jsx") => {
                                            (Language::JavaScript, DocumentKind::Executable)
                                        }
                                        _ => (Language::GraphQL, DocumentKind::Executable),
                                    };
                                files_to_add.push((file_path, content, language, document_kind));
                            }
                        }
                    }
                }
            }

            // Batch add all files for O(n) performance
            let batch_refs: Vec<(FilePath, &str, Language, DocumentKind)> = files_to_add
                .iter()
                .map(|(path, content, language, document_kind)| {
                    (path.clone(), content.as_str(), *language, *document_kind)
                })
                .collect();
            host.add_files_batch(&batch_refs);
        } else {
            // No documents to load, but still need to rebuild for schemas
            host.rebuild_project_files();
        }

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
            .unwrap_or_else(|| "file:///document.graphql".to_string());

        // Determine which project to use (convert to owned to avoid borrow issues)
        let project_name = params
            .project
            .clone()
            .or_else(|| self.first_project().map(ToString::to_string))
            .unwrap_or_else(|| "default".to_string());

        // Add the document to the appropriate host
        if let Some(host) = self.hosts.get_mut(&project_name) {
            let fp = FilePath::new(file_path.clone());
            host.add_file(
                &fp,
                &params.document,
                Language::GraphQL,
                DocumentKind::Executable,
            );
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
                    help: None,
                    url: None,
                    tags: Vec::new(),
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
        let file_path = file_path.unwrap_or("file:///document.graphql");

        // Determine which project to use (convert to owned to avoid borrow issues)
        let project_name = project
            .map(ToString::to_string)
            .or_else(|| self.first_project().map(ToString::to_string))
            .unwrap_or_else(|| "default".to_string());

        // Add the document to the appropriate host
        if let Some(host) = self.hosts.get_mut(&project_name) {
            let fp = FilePath::new(file_path.to_string());
            host.add_file(&fp, document, Language::GraphQL, DocumentKind::Executable);
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
            .map(DiagnosticInfo::from)
            .collect();

        LintResult {
            issue_count,
            fixable_count: 0,
            diagnostics,
        }
    }

    /// Resolve a file path string to a FilePath
    ///
    /// Handles both absolute paths and file:// URIs.
    fn resolve_file_path(file_path: &str) -> FilePath {
        if file_path.contains("://") {
            FilePath::new(file_path.to_string())
        } else {
            FilePath::from_path(std::path::Path::new(file_path))
        }
    }

    /// Resolve which project to use, defaulting to the first loaded project
    fn resolve_project(&self, project: Option<&str>) -> Option<String> {
        project
            .map(ToString::to_string)
            .or_else(|| self.first_project().map(ToString::to_string))
    }

    /// Go to definition of the symbol at the given position
    pub fn goto_definition(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        project: Option<&str>,
    ) -> Option<LocationsResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let fp = Self::resolve_file_path(file_path);
        let position = graphql_ide::Position::new(line, character);

        let locations = analysis.goto_definition(&fp, position)?;
        let results: Vec<LocationResult> =
            locations.into_iter().map(LocationResult::from).collect();
        let count = results.len();
        Some(LocationsResult {
            locations: results,
            count,
        })
    }

    /// Find all references to the symbol at the given position
    pub fn find_references(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
        project: Option<&str>,
    ) -> Option<LocationsResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let fp = Self::resolve_file_path(file_path);
        let position = graphql_ide::Position::new(line, character);

        let locations = analysis.find_references(&fp, position, include_declaration)?;
        let results: Vec<LocationResult> =
            locations.into_iter().map(LocationResult::from).collect();
        let count = results.len();
        Some(LocationsResult {
            locations: results,
            count,
        })
    }

    /// Get hover information for the symbol at the given position
    pub fn hover(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        project: Option<&str>,
    ) -> Option<HoverResultInfo> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let fp = Self::resolve_file_path(file_path);
        let position = graphql_ide::Position::new(line, character);

        let hover = analysis.hover(&fp, position)?;
        Some(HoverResultInfo {
            contents: hover.contents,
            range: hover.range.map(Into::into),
        })
    }

    /// Get document symbols (outline) for a file
    pub fn document_symbols(
        &self,
        file_path: &str,
        project: Option<&str>,
    ) -> Option<DocumentSymbolsResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let fp = Self::resolve_file_path(file_path);

        let symbols = analysis.document_symbols(&fp);
        let results: Vec<SymbolInfo> = symbols.into_iter().map(SymbolInfo::from).collect();
        let count = results.len();
        Some(DocumentSymbolsResult {
            symbols: results,
            count,
        })
    }

    /// Search for symbols across the workspace
    pub fn workspace_symbols(
        &self,
        query: &str,
        project: Option<&str>,
    ) -> Option<WorkspaceSymbolsResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;

        let symbols = analysis.workspace_symbols(query);
        let results: Vec<WorkspaceSymbolInfo> =
            symbols.into_iter().map(WorkspaceSymbolInfo::from).collect();
        let count = results.len();
        Some(WorkspaceSymbolsResult {
            symbols: results,
            count,
        })
    }

    /// Get completions at the given position
    pub fn completions(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        project: Option<&str>,
    ) -> Option<CompletionsResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let fp = Self::resolve_file_path(file_path);
        let position = graphql_ide::Position::new(line, character);

        let items = analysis.completions(&fp, position)?;
        let results: Vec<CompletionInfo> = items.into_iter().map(CompletionInfo::from).collect();
        let count = results.len();
        Some(CompletionsResult {
            items: results,
            count,
        })
    }

    /// Get diagnostics for a specific file
    pub fn file_diagnostics(
        &self,
        file_path: &str,
        project: Option<&str>,
    ) -> Option<FileDiagnostics> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let fp = Self::resolve_file_path(file_path);

        let diagnostics = analysis.all_diagnostics_for_file(&fp);
        Some(FileDiagnostics {
            file: file_path.to_string(),
            diagnostics: diagnostics.into_iter().map(DiagnosticInfo::from).collect(),
        })
    }

    /// List all schema types with metadata
    pub fn schema_types(
        &self,
        kind_filter: Option<&str>,
        project: Option<&str>,
    ) -> Option<SchemaTypesResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;

        let (entries, stats) = analysis.schema_type_list(kind_filter);
        let count = entries.len();
        let types = entries
            .into_iter()
            .map(|e| SchemaTypeInfo {
                name: e.name,
                kind: e.kind,
                description: e.description,
                field_count: e.field_count,
                implements: e.implements,
                is_extension: e.is_extension,
            })
            .collect();

        Some(SchemaTypesResult {
            types,
            count,
            stats: SchemaStatsInfo {
                objects: stats.objects,
                interfaces: stats.interfaces,
                unions: stats.unions,
                enums: stats.enums,
                scalars: stats.scalars,
                input_objects: stats.input_objects,
                total_fields: stats.total_fields,
                directives: stats.directives,
            },
        })
    }

    /// Get full details about a specific type
    pub fn type_info(&self, type_name: &str, project: Option<&str>) -> Option<TypeInfoResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let info = analysis.type_info(type_name)?;

        Some(TypeInfoResult {
            name: info.name,
            kind: info.kind,
            description: info.description,
            implements: info.implements,
            fields: info
                .fields
                .into_iter()
                .map(|f| FieldInfo {
                    name: f.name,
                    type_ref: f.type_ref,
                    description: f.description,
                    arguments: f
                        .arguments
                        .into_iter()
                        .map(|a| ArgumentInfo {
                            name: a.name,
                            type_ref: a.type_ref,
                            description: a.description,
                            default_value: a.default_value,
                        })
                        .collect(),
                    is_deprecated: f.is_deprecated,
                    deprecation_reason: f.deprecation_reason,
                    directives: f
                        .directives
                        .into_iter()
                        .map(|d| DirectiveInfo {
                            name: d.name,
                            arguments: d
                                .arguments
                                .into_iter()
                                .map(|a| DirectiveArgumentInfo {
                                    name: a.name,
                                    value: a.value,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            directives: info
                .directives
                .into_iter()
                .map(|d| DirectiveInfo {
                    name: d.name,
                    arguments: d
                        .arguments
                        .into_iter()
                        .map(|a| DirectiveArgumentInfo {
                            name: a.name,
                            value: a.value,
                        })
                        .collect(),
                })
                .collect(),
            enum_values: info
                .enum_values
                .into_iter()
                .map(|v| EnumValueInfo {
                    name: v.name,
                    description: v.description,
                    is_deprecated: v.is_deprecated,
                    deprecation_reason: v.deprecation_reason,
                })
                .collect(),
            union_members: info.union_members,
        })
    }

    /// Get the full merged schema SDL
    pub fn schema_sdl(&self, project: Option<&str>) -> Option<SchemaSdlResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;
        let (entries, _) = analysis.schema_type_list(None);
        let type_count = entries.len();

        // Access the HIR types directly for SDL printing
        let sdl = analysis.with_schema_types(crate::sdl_printer::print_schema_sdl);

        Some(SchemaSdlResult { sdl, type_count })
    }

    /// Extract operations from the project
    pub fn operations(
        &self,
        file_path: Option<&str>,
        project: Option<&str>,
    ) -> Option<OperationsResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;

        let file_filter = file_path.map(Self::resolve_file_path);
        let summaries = analysis.operations_summary(file_filter.as_ref());
        let count = summaries.len();

        let operations = summaries
            .into_iter()
            .map(|s| OperationInfo {
                name: s.name,
                operation_type: s.operation_type,
                file: s.file.as_str().to_string(),
                variables: s
                    .variables
                    .into_iter()
                    .map(|v| VariableInfo {
                        name: v.name,
                        type_ref: v.type_ref,
                        default_value: v.default_value,
                    })
                    .collect(),
                fragment_dependencies: s.fragment_dependencies,
            })
            .collect();

        Some(OperationsResult { operations, count })
    }

    /// Get complexity analysis for operations
    pub fn query_complexity(
        &self,
        operation_name: Option<&str>,
        project: Option<&str>,
    ) -> Option<QueryComplexityResult> {
        let project_name = self.resolve_project(project)?;
        let analysis = self.analysis(&project_name)?;

        let all = analysis.complexity_analysis();
        let filtered: Vec<_> = if let Some(name) = operation_name {
            all.into_iter()
                .filter(|c| c.operation_name == name)
                .collect()
        } else {
            all
        };

        let count = filtered.len();
        let operations = filtered
            .into_iter()
            .map(|c| ComplexityInfo {
                operation_name: c.operation_name,
                operation_type: c.operation_type,
                total_complexity: c.total_complexity,
                depth: c.depth,
                breakdown: c
                    .breakdown
                    .into_iter()
                    .map(|b| FieldComplexityInfo {
                        path: b.path,
                        complexity: b.complexity,
                        multiplier: if b.multiplier > 1 {
                            Some(b.multiplier)
                        } else {
                            None
                        },
                    })
                    .collect(),
                warnings: c.warnings,
                file: c.file.as_str().to_string(),
            })
            .collect();

        Some(QueryComplexityResult { operations, count })
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
            &FilePath::new("file:///test/schema.graphql".to_string()),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
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
            &FilePath::new("file:///test/schema.graphql".to_string()),
            "type Query { users: [User] } type User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host1.rebuild_project_files();

        let host2 = service.get_or_create_host("project2");
        host2.add_file(
            &FilePath::new("file:///test/schema.graphql".to_string()),
            "type Query { posts: [Post] } type Post { title: String }",
            Language::GraphQL,
            DocumentKind::Schema,
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

    fn setup_service_with_documents(schema: &str, doc_path: &str, doc: &str) -> McpService {
        let mut service = McpService::new();
        let host = service.get_or_create_host("default");
        host.add_file(
            &FilePath::new("file:///test/schema.graphql".to_string()),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.add_file(
            &FilePath::new(doc_path.to_string()),
            doc,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();
        service
    }

    #[test]
    fn test_hover() {
        let service = setup_service_with_documents(
            "type Query { hello: String }",
            "file:///test/query.graphql",
            "query { hello }",
        );

        // Hover over "hello" field (line 0, character 8)
        let result = service.hover("file:///test/query.graphql", 0, 8, None);
        assert!(result.is_some());
        let hover = result.unwrap();
        assert!(!hover.contents.is_empty());
    }

    #[test]
    fn test_hover_no_result() {
        let service = setup_service_with_documents(
            "type Query { hello: String }",
            "file:///test/query.graphql",
            "query { hello }",
        );

        // Hover over whitespace (should return None)
        let result = service.hover("file:///nonexistent.graphql", 0, 0, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_document_symbols() {
        let service = setup_service_with_documents(
            "type Query { hello: String }",
            "file:///test/query.graphql",
            "query GetHello { hello }",
        );

        let result = service.document_symbols("file:///test/query.graphql", None);
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert!(symbols.count > 0);
        assert_eq!(symbols.symbols[0].name, "GetHello");
        assert_eq!(symbols.symbols[0].kind, "query");
    }

    #[test]
    fn test_document_symbols_schema() {
        let service = setup_service_with_schema(
            "type Query { hello: String }\ntype User { id: ID! name: String }",
        );

        let result = service.document_symbols("file:///test/schema.graphql", None);
        assert!(result.is_some());
        let symbols = result.unwrap();
        // Should have Query and User types
        assert!(symbols.count >= 2);
    }

    #[test]
    fn test_workspace_symbols() {
        let service = setup_service_with_documents(
            "type Query { hello: String }\ntype User { id: ID! }",
            "file:///test/query.graphql",
            "query GetHello { hello }",
        );

        let result = service.workspace_symbols("User", None);
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert!(symbols.count > 0);
        assert!(symbols.symbols.iter().any(|s| s.name == "User"));
    }

    #[test]
    fn test_completions() {
        let service = setup_service_with_documents(
            "type Query { hello: String, world: Int }",
            "file:///test/query.graphql",
            "query { }",
        );

        // Position inside the selection set (after opening brace)
        let result = service.completions("file:///test/query.graphql", 0, 8, None);
        assert!(result.is_some());
        let completions = result.unwrap();
        assert!(completions.count > 0);
        assert!(completions.items.iter().any(|c| c.label == "hello"));
    }

    #[test]
    fn test_file_diagnostics() {
        let service = setup_service_with_documents(
            "type Query { hello: String }",
            "file:///test/query.graphql",
            "query { nonexistent }",
        );

        let result = service.file_diagnostics("file:///test/query.graphql", None);
        assert!(result.is_some());
        let diagnostics = result.unwrap();
        assert!(!diagnostics.diagnostics.is_empty());
    }

    #[test]
    fn test_file_diagnostics_clean() {
        let service = setup_service_with_documents(
            "type Query { hello: String }",
            "file:///test/query.graphql",
            "query { hello }",
        );

        let result = service.file_diagnostics("file:///test/query.graphql", None);
        assert!(result.is_some());
        let diagnostics = result.unwrap();
        assert!(diagnostics.diagnostics.is_empty());
    }

    #[test]
    fn test_goto_definition() {
        let service = setup_service_with_documents(
            "type Query { hello: String }",
            "file:///test/query.graphql",
            "query { hello }",
        );

        // "hello" starts at character 8 in the query
        let result = service.goto_definition("file:///test/query.graphql", 0, 8, None);
        assert!(result.is_some());
        let locations = result.unwrap();
        assert!(locations.count > 0);
        // Should point to the schema definition
        assert!(locations.locations[0]
            .file
            .contains("file:///test/schema.graphql"));
    }

    #[test]
    fn test_find_references_fragment() {
        let service = {
            let mut service = McpService::new();
            let host = service.get_or_create_host("default");
            host.add_file(
                &FilePath::new("file:///test/schema.graphql".to_string()),
                "type Query { user: User }\ntype User { id: ID!, name: String }",
                Language::GraphQL,
                DocumentKind::Schema,
            );
            host.add_file(
                &FilePath::new("file:///test/fragment.graphql".to_string()),
                "fragment UserFields on User { id name }",
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.add_file(
                &FilePath::new("file:///test/query.graphql".to_string()),
                "query { user { ...UserFields } }",
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();
            service
        };

        // Find references to the fragment name "UserFields" (at position 9 in fragment.graphql)
        let result = service.find_references("file:///test/fragment.graphql", 0, 10, true, None);
        assert!(result.is_some());
        let locations = result.unwrap();
        // Should find the definition and the spread
        assert!(locations.count >= 2);
    }

    #[test]
    fn test_resolve_file_path_uri() {
        let fp = McpService::resolve_file_path("file:///home/user/query.graphql");
        assert_eq!(fp.as_str(), "file:///home/user/query.graphql");
    }

    #[test]
    fn test_resolve_file_path_absolute() {
        let fp = McpService::resolve_file_path("/home/user/query.graphql");
        assert_eq!(fp.as_str(), "file:///home/user/query.graphql");
    }

    // --- Schema exploration tests ---

    #[test]
    fn test_schema_types_lists_all() {
        let service = setup_service_with_schema(
            "type Query { user: User }
             type User { id: ID!, name: String }
             enum Status { ACTIVE INACTIVE }
             input CreateUserInput { name: String! }",
        );

        let result = service.schema_types(None, None).unwrap();
        assert!(result.count >= 4); // Query, User, Status, CreateUserInput
        assert!(result
            .types
            .iter()
            .any(|t| t.name == "User" && t.kind == "object"));
        assert!(result
            .types
            .iter()
            .any(|t| t.name == "Status" && t.kind == "enum"));
        assert!(result
            .types
            .iter()
            .any(|t| t.name == "CreateUserInput" && t.kind == "input_object"));
    }

    #[test]
    fn test_schema_types_filter_by_kind() {
        let service = setup_service_with_schema(
            "type Query { user: User }
             type User { id: ID! }
             enum Status { ACTIVE }",
        );

        let result = service.schema_types(Some("enum"), None).unwrap();
        assert!(result.types.iter().all(|t| t.kind == "enum"));
        assert!(result.types.iter().any(|t| t.name == "Status"));
    }

    #[test]
    fn test_schema_types_stats() {
        let service = setup_service_with_schema(
            "type Query { user: User }
             type User { id: ID!, name: String }
             interface Node { id: ID! }
             union SearchResult = User
             enum Status { ACTIVE }
             scalar DateTime
             input CreateUserInput { name: String! }",
        );

        let result = service.schema_types(None, None).unwrap();
        assert!(result.stats.objects >= 2); // Query + User
        assert!(result.stats.interfaces >= 1);
        assert!(result.stats.unions >= 1);
        assert!(result.stats.enums >= 1);
        assert!(result.stats.scalars >= 1);
        assert!(result.stats.input_objects >= 1);
    }

    #[test]
    fn test_type_info_object() {
        let service = setup_service_with_schema(
            "type Query { user(id: ID!): User }
             type User implements Node { id: ID!, name: String, email: String }
             interface Node { id: ID! }",
        );

        let result = service.type_info("User", None).unwrap();
        assert_eq!(result.name, "User");
        assert_eq!(result.kind, "object");
        assert_eq!(result.implements, vec!["Node"]);
        assert!(result.fields.len() >= 3);
        assert!(result
            .fields
            .iter()
            .any(|f| f.name == "id" && f.type_ref == "ID!"));
        assert!(result.fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_type_info_enum() {
        let service = setup_service_with_schema(
            "type Query { status: Status }
             enum Status { ACTIVE INACTIVE PENDING }",
        );

        let result = service.type_info("Status", None).unwrap();
        assert_eq!(result.kind, "enum");
        assert_eq!(result.enum_values.len(), 3);
        assert!(result.enum_values.iter().any(|v| v.name == "ACTIVE"));
    }

    #[test]
    fn test_type_info_union() {
        let service = setup_service_with_schema(
            "type Query { search: SearchResult }
             union SearchResult = User | Post
             type User { id: ID! }
             type Post { title: String }",
        );

        let result = service.type_info("SearchResult", None).unwrap();
        assert_eq!(result.kind, "union");
        assert_eq!(result.union_members.len(), 2);
        assert!(result.union_members.contains(&"User".to_string()));
        assert!(result.union_members.contains(&"Post".to_string()));
    }

    #[test]
    fn test_type_info_not_found() {
        let service = setup_service_with_schema("type Query { hello: String }");
        let result = service.type_info("NonExistent", None);
        assert!(result.is_none());
    }

    #[test]
    fn test_type_info_field_arguments() {
        let service = setup_service_with_schema(
            "type Query { users(first: Int = 10, after: String): [User] }
             type User { id: ID! }",
        );

        let result = service.type_info("Query", None).unwrap();
        let users_field = result.fields.iter().find(|f| f.name == "users").unwrap();
        assert_eq!(users_field.arguments.len(), 2);
        let first_arg = users_field
            .arguments
            .iter()
            .find(|a| a.name == "first")
            .unwrap();
        assert_eq!(first_arg.type_ref, "Int");
        assert_eq!(first_arg.default_value.as_deref(), Some("10"));
    }

    #[test]
    fn test_schema_sdl() {
        let service = setup_service_with_schema(
            "type Query { user: User }
             type User { id: ID!, name: String }",
        );

        let result = service.schema_sdl(None).unwrap();
        assert!(result.sdl.contains("type Query"));
        assert!(result.sdl.contains("type User"));
        assert!(result.sdl.contains("id: ID!"));
        assert!(result.type_count >= 2);
    }

    #[test]
    fn test_schema_sdl_enum() {
        let service = setup_service_with_schema(
            "type Query { status: Status }
             enum Status { ACTIVE INACTIVE }",
        );

        let result = service.schema_sdl(None).unwrap();
        assert!(result.sdl.contains("enum Status"));
        assert!(result.sdl.contains("ACTIVE"));
        assert!(result.sdl.contains("INACTIVE"));
    }

    // --- Document analysis tests ---

    #[test]
    fn test_operations() {
        let service = setup_service_with_documents(
            "type Query { user(id: ID!): User }
             type User { id: ID!, name: String }",
            "file:///test/query.graphql",
            "query GetUser($id: ID!) { user(id: $id) { id name } }
             mutation { __typename }",
        );

        let result = service.operations(None, None).unwrap();
        assert!(result.count >= 1);
        let get_user = result
            .operations
            .iter()
            .find(|o| o.name.as_deref() == Some("GetUser"));
        assert!(get_user.is_some());
        let get_user = get_user.unwrap();
        assert_eq!(get_user.operation_type, "query");
        assert_eq!(get_user.variables.len(), 1);
        assert_eq!(get_user.variables[0].name, "id");
        assert_eq!(get_user.variables[0].type_ref, "ID!");
    }

    #[test]
    fn test_operations_with_fragments() {
        let service = {
            let mut service = McpService::new();
            let host = service.get_or_create_host("default");
            host.add_file(
                &FilePath::new("file:///test/schema.graphql".to_string()),
                "type Query { user: User }\ntype User { id: ID!, name: String }",
                Language::GraphQL,
                DocumentKind::Schema,
            );
            host.add_file(
                &FilePath::new("file:///test/fragment.graphql".to_string()),
                "fragment UserFields on User { id name }",
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.add_file(
                &FilePath::new("file:///test/query.graphql".to_string()),
                "query GetUser { user { ...UserFields } }",
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();
            service
        };

        let result = service.operations(None, None).unwrap();
        let get_user = result
            .operations
            .iter()
            .find(|o| o.name.as_deref() == Some("GetUser"))
            .unwrap();
        assert!(get_user
            .fragment_dependencies
            .contains(&"UserFields".to_string()));
    }

    #[test]
    fn test_operations_filter_by_file() {
        let service = {
            let mut service = McpService::new();
            let host = service.get_or_create_host("default");
            host.add_file(
                &FilePath::new("file:///test/schema.graphql".to_string()),
                "type Query { a: String, b: String }",
                Language::GraphQL,
                DocumentKind::Schema,
            );
            host.add_file(
                &FilePath::new("file:///test/a.graphql".to_string()),
                "query A { a }",
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.add_file(
                &FilePath::new("file:///test/b.graphql".to_string()),
                "query B { b }",
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();
            service
        };

        let result = service
            .operations(Some("file:///test/a.graphql"), None)
            .unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.operations[0].name.as_deref(), Some("A"));
    }

    #[test]
    fn test_query_complexity() {
        let service = setup_service_with_documents(
            "type Query { user: User }
             type User { id: ID!, name: String, posts: [Post] }
             type Post { id: ID!, title: String }",
            "file:///test/query.graphql",
            "query GetUser { user { id name posts { id title } } }",
        );

        let result = service.query_complexity(None, None).unwrap();
        assert!(result.count >= 1);
        let get_user = result
            .operations
            .iter()
            .find(|o| o.operation_name == "GetUser")
            .unwrap();
        assert!(get_user.total_complexity > 0);
        assert!(get_user.depth > 0);
    }

    #[test]
    fn test_query_complexity_filter_by_name() {
        let service = setup_service_with_documents(
            "type Query { a: String, b: String }",
            "file:///test/query.graphql",
            "query A { a }\nquery B { b }",
        );

        let result = service.query_complexity(Some("A"), None).unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.operations[0].operation_name, "A");
    }
}
