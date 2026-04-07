use crate::handlers;

use crate::conversions::convert_ide_diagnostic;
use crate::workspace::{ProjectHost, WorkspaceManager};
use graphql_config::find_config;
use lsp_types::{
    ClientCapabilities, CodeActionKind, CodeActionOptions, CodeActionParams, CodeActionResponse,
    CodeLens, CodeLensOptions, CodeLensParams, CompletionOptions, CompletionParams,
    CompletionResponse, Diagnostic, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentSymbolParams, DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams,
    FileSystemWatcher, FoldingRange, FoldingRangeParams, FoldingRangeProviderCapability,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, InlayHint as LspInlayHint,
    InlayHintOptions, InlayHintParams, InlayHintServerCapabilities, Location, MessageActionItem,
    MessageType, OneOf, PrepareRenameResponse, ReferenceParams, RenameOptions, RenameParams,
    SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability, SemanticTokenModifier,
    SemanticTokenType, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, ShowDocumentParams, SignatureHelpOptions, SignatureHelpParams,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    WorkDoneProgressOptions, WorkspaceEdit, WorkspaceSymbolParams,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types as lsp_types;
use tower_lsp_server::{Client, LanguageServer};

/// Parameters for the `graphql/virtualFileContent` custom request.
///
/// This request fetches the content of virtual files (like introspected remote schemas)
/// that don't exist on disk but are registered in the LSP's file registry.
#[derive(Debug, serde::Deserialize)]
pub struct VirtualFileContentParams {
    /// The URI of the virtual file to fetch (e.g., `schema://api.example.com/graphql/schema.graphql`)
    pub uri: String,
}

/// Custom notification sent from server to client to indicate loading status.
/// The extension uses this to update the status bar (spinning icon during loading,
/// checkmark when ready).
pub enum StatusNotification {}

impl lsp_types::notification::Notification for StatusNotification {
    type Params = StatusParams;
    const METHOD: &'static str = "graphql-analyzer/status";
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StatusParams {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response for the `graphql/ping` health check request.
#[derive(Debug, serde::Serialize)]
pub struct PingResponse {
    /// Server timestamp in milliseconds since Unix epoch.
    pub timestamp: u64,
}

pub struct GraphQLLanguageServer {
    pub(crate) client: Client,
    /// Client capabilities received during initialization
    pub(crate) client_capabilities: Arc<RwLock<Option<ClientCapabilities>>>,
    /// Workspace manager for all workspace/project state
    pub(crate) workspace: Arc<WorkspaceManager>,
    /// Trace capture manager (None if tracing init failed)
    pub(crate) trace_capture: Option<Arc<crate::trace_capture::TraceCaptureManager>>,
}

/// Background task that loads workspace configs and publishes initial diagnostics.
/// Runs asynchronously so the LSP can respond to requests during loading.
async fn load_workspaces_background(
    client: Client,
    workspace: Arc<WorkspaceManager>,
    folders: Vec<(String, PathBuf)>,
) {
    let loading_start = std::time::Instant::now();

    tracing::debug!(
        "Loading configs for {} workspace(s) in background",
        folders.len()
    );

    client
        .send_notification::<StatusNotification>(StatusParams {
            status: "loading".to_string(),
            message: Some(format!("Loading {} workspace(s)...", folders.len())),
        })
        .await;

    for (uri, path) in folders {
        tracing::debug!(
            "Loading config for workspace: {} at {}",
            uri,
            path.display()
        );
        load_workspace_config_background(&client, &workspace, &uri, &path).await;
    }

    tracing::debug!(
        "Background loading complete: {} workspace roots, {} configs",
        workspace.workspace_roots.len(),
        workspace.configs.len()
    );

    let elapsed = loading_start.elapsed();
    let total_files = workspace.file_to_project.len();

    let init_message = format!(
        "Project initialization complete: {} files loaded in {:.1}s",
        total_files,
        elapsed.as_secs_f64()
    );
    tracing::info!("{}", init_message);
    client.log_message(MessageType::INFO, &init_message).await;

    client
        .send_notification::<StatusNotification>(StatusParams {
            status: "ready".to_string(),
            message: Some(format!(
                "{} files loaded in {:.1}s",
                total_files,
                elapsed.as_secs_f64()
            )),
        })
        .await;

    // Register file watchers for config files after loading
    let config_paths: Vec<PathBuf> = workspace
        .config_paths
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    if config_paths.is_empty() {
        tracing::debug!("No config paths found to watch");
        return;
    }

    tracing::debug!(
        count = config_paths.len(),
        "Registering config file watchers"
    );

    let watchers: Vec<FileSystemWatcher> = config_paths
        .iter()
        .filter_map(|path| {
            let filename = path.file_name()?.to_str()?;
            Some(FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::String(format!("**/{filename}")),
                kind: Some(lsp_types::WatchKind::all()),
            })
        })
        .collect();

    let registration = lsp_types::Registration {
        id: "graphql-config-watcher".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(
            serde_json::to_value(lsp_types::DidChangeWatchedFilesRegistrationOptions { watchers })
                .expect("DidChangeWatchedFilesRegistrationOptions is always serializable"),
        ),
    };

    if let Err(e) = client.register_capability(vec![registration]).await {
        tracing::error!("Failed to register config file watchers: {:?}", e);
    }
}

/// Convert config validation errors to LSP diagnostics.
fn validation_errors_to_diagnostics(
    errors: &[graphql_config::ConfigValidationError],
    config_content: &str,
) -> Vec<Diagnostic> {
    errors
        .iter()
        .map(|error| {
            let range = error
                .location(config_content)
                .map_or(lsp_types::Range::default(), |loc| lsp_types::Range {
                    start: lsp_types::Position {
                        line: loc.line,
                        character: loc.start_column,
                    },
                    end: lsp_types::Position {
                        line: loc.line,
                        character: loc.end_column,
                    },
                });

            let severity = match error.severity() {
                graphql_config::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
                graphql_config::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            };

            Diagnostic {
                range,
                severity: Some(severity),
                code: Some(lsp_types::NumberOrString::String(error.code().to_string())),
                source: Some("graphql-config".to_string()),
                message: error.message(),
                ..Default::default()
            }
        })
        .collect()
}

/// Load a single workspace config in the background
async fn load_workspace_config_background(
    client: &Client,
    workspace: &Arc<WorkspaceManager>,
    workspace_uri: &str,
    workspace_path: &Path,
) {
    workspace
        .workspace_roots
        .insert(workspace_uri.to_string(), workspace_path.to_path_buf());

    match find_config(workspace_path) {
        Ok(Some(config_path)) => {
            workspace
                .config_paths
                .insert(workspace_uri.to_string(), config_path.clone());

            match graphql_config::load_config(&config_path) {
                Ok(config) => {
                    // Validate configuration and always publish diagnostics
                    // (empty list clears previous errors when config becomes valid)
                    let lint_rule_names = graphql_linter::all_rule_names();
                    let lint_context = graphql_config::LintValidationContext {
                        valid_rule_names: &lint_rule_names,
                        valid_presets: &["recommended"],
                    };
                    let errors =
                        graphql_config::validate(&config, workspace_path, Some(&lint_context));
                    let config_uri = Uri::from_str(&graphql_ide::path_to_file_uri(&config_path))
                        .expect("valid config path");

                    let has_errors = errors
                        .iter()
                        .any(|e| e.severity() == graphql_config::Severity::Error);

                    if errors.is_empty() {
                        // Config is valid -- clear any previous validation errors
                        client.publish_diagnostics(config_uri, vec![], None).await;
                    } else {
                        let config_content =
                            std::fs::read_to_string(&config_path).unwrap_or_default();
                        let diagnostics =
                            validation_errors_to_diagnostics(&errors, &config_content);
                        client
                            .publish_diagnostics(config_uri.clone(), diagnostics, None)
                            .await;

                        if has_errors {
                            let error_count = errors
                                .iter()
                                .filter(|e| e.severity() == graphql_config::Severity::Error)
                                .count();

                            let actions = vec![
                                MessageActionItem {
                                    title: "Open Config".to_string(),
                                    properties: HashMap::default(),
                                },
                                MessageActionItem {
                                    title: "Dismiss".to_string(),
                                    properties: HashMap::default(),
                                },
                            ];

                            let response = client
                                .show_message_request(
                                    MessageType::ERROR,
                                    format!(
                                        "GraphQL config has {error_count} validation error(s). \
                                        Please fix the configuration before continuing.",
                                    ),
                                    Some(actions),
                                )
                                .await;

                            if let Ok(Some(action)) = response {
                                if action.title == "Open Config" {
                                    let _ = client
                                        .show_document(ShowDocumentParams {
                                            uri: config_uri,
                                            external: Some(false),
                                            take_focus: Some(true),
                                            selection: None,
                                        })
                                        .await;
                                }
                            }

                            return;
                        }
                    }

                    client
                        .log_message(MessageType::INFO, "GraphQL config found, loading files...")
                        .await;

                    workspace
                        .configs
                        .insert(workspace_uri.to_string(), config.clone());

                    load_all_project_files_background(
                        client,
                        workspace,
                        workspace_uri,
                        workspace_path,
                        &config,
                        &config_path,
                    )
                    .await;
                }
                Err(e) => {
                    tracing::error!("Error loading config: {}", e);
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load GraphQL config: {e}"),
                        )
                        .await;
                }
            }
        }
        Ok(None) => {
            // In background mode, just log - don't show interactive dialog
            tracing::debug!("No GraphQL config found in workspace");
            client
                .log_message(
                    MessageType::INFO,
                    "No GraphQL config found. Create a .graphqlrc.yaml for full IDE features.",
                )
                .await;
        }
        Err(e) => {
            tracing::error!("Error searching for config: {}", e);
        }
    }
}

/// Load all project files in the background
async fn load_all_project_files_background(
    client: &Client,
    workspace: &Arc<WorkspaceManager>,
    workspace_uri: &str,
    workspace_path: &Path,
    config: &graphql_config::GraphQLConfig,
    config_path: &Path,
) {
    let start = std::time::Instant::now();
    let projects: Vec<_> = config.projects().collect();
    tracing::debug!(
        "Loading files for {} project(s) in background",
        projects.len()
    );

    // Collect all content mismatch errors across all projects
    let mut content_mismatch_errors: Vec<graphql_config::ConfigValidationError> = Vec::new();
    // Track projects and their unmatched schema patterns
    // Each tuple is (project_name, unmatched_patterns, has_no_schema)
    let mut schema_pattern_results: Vec<(String, Vec<String>, bool)> = Vec::new();
    // Track projects and their unmatched document patterns
    // Each tuple is (project_name, unmatched_patterns, has_no_documents)
    let mut documents_pattern_results: Vec<(String, Vec<String>, bool)> = Vec::new();

    for (project_name, project_config) in projects {
        let project_start = std::time::Instant::now();
        tracing::debug!("Loading project: {}", project_name);

        let extract_config = project_config
            .extensions
            .as_ref()
            .and_then(|extensions| extensions.get("extractConfig"))
            .and_then(|extract_config_value| {
                serde_json::from_value::<graphql_extract::ExtractConfig>(
                    extract_config_value.clone(),
                )
                .ok()
            })
            .unwrap_or_default();

        let lint_config =
            project_config
                .lint()
                .map_or_else(graphql_linter::LintConfig::default, |lint_value| {
                    serde_json::from_value::<graphql_linter::LintConfig>(lint_value.clone())
                        .unwrap_or_default()
                });

        let host = workspace.get_or_create_host(workspace_uri, project_name);

        host.with_write(|h| {
            h.set_extract_config(extract_config.clone());
            h.set_lint_config(lint_config);
        })
        .await;

        // Load schemas
        let (pending_introspections, no_user_schema, unmatched_patterns, schema_count) = host
            .with_write(
                |h| match h.load_schemas_from_config(project_config, workspace_path) {
                    Ok(result) => {
                        tracing::debug!(
                            "Loaded {} local schema file(s), {} remote pending",
                            result.loaded_count,
                            result.pending_introspections.len()
                        );
                        let no_schema = result.has_no_user_schema();
                        let count = result.loaded_count;
                        // Convert content mismatch errors to ConfigValidationError
                        for error in &result.content_errors {
                            tracing::warn!(
                                "Content mismatch in '{}': file in schema config contains executable definitions: {}",
                                error.file_path.display(),
                                error.unexpected_definitions.join(", ")
                            );
                            content_mismatch_errors.push(
                                graphql_config::ConfigValidationError::ContentMismatch {
                                    project: project_name.to_string(),
                                    pattern: error.pattern.clone(),
                                    expected: graphql_config::FileType::Schema,
                                    file_path: error.file_path.clone(),
                                    unexpected_definitions: error.unexpected_definitions.clone(),
                                },
                            );
                        }
                        (result.pending_introspections, no_schema, result.unmatched_patterns, count)
                    }
                    Err(e) => {
                        tracing::error!("Failed to load schemas: {}", e);
                        (vec![], true, vec![], 0)
                    }
                },
            )
            .await;

        // Track unmatched patterns for diagnostics
        if !unmatched_patterns.is_empty() || no_user_schema {
            schema_pattern_results.push((
                project_name.to_string(),
                unmatched_patterns.clone(),
                no_user_schema,
            ));
        }

        if no_user_schema {
            tracing::warn!(
                "Project '{}': no schema files found matching configured patterns",
                project_name
            );
            client
                .show_message(
                    MessageType::WARNING,
                    format!("GraphQL: No schema files found for project '{project_name}'. Schema validation will be skipped."),
                )
                .await;
        }

        // Fetch remote schemas (if any)
        for pending in &pending_introspections {
            let mut introspect_client = graphql_introspect::IntrospectionClient::new();
            if let Some(headers) = &pending.headers {
                for (name, value) in headers {
                    introspect_client = introspect_client.with_header(name, value);
                }
            }
            if let Some(timeout) = pending.timeout {
                introspect_client = introspect_client.with_timeout(Duration::from_secs(timeout));
            }

            match introspect_client.execute(&pending.url).await {
                Ok(response) => {
                    let sdl = graphql_introspect::introspection_to_sdl(&response);
                    host.with_write(|h| h.add_introspected_schema(&pending.url, &sdl))
                        .await;
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Loaded remote schema from {}", pending.url),
                        )
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to introspect {}: {}", pending.url, e);
                }
            }
        }

        // Phase 1: Discover and read files (no lock needed - just file I/O)
        let discovery_result =
            graphql_ide::discover_document_files(project_config, workspace_path, &extract_config);

        // Convert content mismatch errors to ConfigValidationError
        for error in &discovery_result.errors {
            tracing::warn!(
                "Content mismatch in '{}': file in documents config contains schema definitions: {}",
                error.file_path.display(),
                error.unexpected_definitions.join(", ")
            );
            content_mismatch_errors.push(graphql_config::ConfigValidationError::ContentMismatch {
                project: project_name.to_string(),
                pattern: error.pattern.clone(),
                expected: graphql_config::FileType::Document,
                file_path: error.file_path.clone(),
                unexpected_definitions: error.unexpected_definitions.clone(),
            });
        }

        // Track unmatched document patterns for diagnostics
        let has_no_documents = discovery_result.files.is_empty();
        if !discovery_result.unmatched_patterns.is_empty() || has_no_documents {
            // Only track if documents config exists
            if project_config.documents.is_some() {
                documents_pattern_results.push((
                    project_name.to_string(),
                    discovery_result.unmatched_patterns.clone(),
                    has_no_documents,
                ));
            }
        }

        if has_no_documents {
            if project_config.documents.is_some() {
                tracing::warn!(
                    "Project '{}': no document files found matching configured patterns",
                    project_name
                );
            }
            continue;
        }

        let total_files = discovery_result.files.len();
        tracing::debug!(
            "Discovered {} document files for project '{}'",
            total_files,
            project_name
        );

        // Phase 2: Register files (brief lock acquisition)
        let loaded_files = host
            .with_write(|h| h.add_discovered_files(&discovery_result.files))
            .await;

        // Register files in workspace index (no lock needed)
        for loaded_file in &loaded_files {
            workspace.file_to_project.insert(
                loaded_file.path.as_str().to_string(),
                (workspace_uri.to_string(), project_name.to_string()),
            );
        }

        // Rebuild project files index (brief lock)
        host.with_write(graphql_ide::AnalysisHost::rebuild_project_files)
            .await;

        // Compute and publish diagnostics
        // Use try_snapshot since this is a background task - if we can't get the lock, just skip diagnostics
        let Some(snapshot) = host.try_snapshot().await else {
            tracing::warn!("Could not get snapshot for diagnostics in background load");
            continue;
        };
        let loaded_file_paths: Vec<graphql_ide::FilePath> =
            loaded_files.iter().map(|f| f.path.clone()).collect();

        let all_diagnostics_map = GraphQLLanguageServer::blocking(move || {
            let diagnostics = snapshot.all_diagnostics_for_files(&loaded_file_paths);

            // Prewarm field coverage analysis while on the blocking thread —
            // this triggers operation_body() for all operations so the first
            // hover doesn't block on a project-wide query.
            let prewarm_start = std::time::Instant::now();
            let _ = snapshot.field_coverage();
            tracing::debug!(
                "Prewarmed field coverage in {:.2}ms",
                prewarm_start.elapsed().as_secs_f64() * 1000.0
            );

            diagnostics
        })
        .await;

        let Some(all_diagnostics_map) = all_diagnostics_map else {
            tracing::warn!("Background diagnostics task panicked");
            continue;
        };

        tracing::debug!(
            "Publishing diagnostics for {} files with issues",
            all_diagnostics_map.len()
        );

        for (file_path, diagnostics) in &all_diagnostics_map {
            let Ok(file_uri) = Uri::from_str(file_path.as_str()) else {
                continue;
            };
            let lsp_diagnostics: Vec<Diagnostic> = diagnostics
                .iter()
                .cloned()
                .map(convert_ide_diagnostic)
                .collect();
            client
                .publish_diagnostics(file_uri, lsp_diagnostics, None)
                .await;
        }

        let project_msg = format!(
            "Project '{}' loaded: {} schema file(s), {} document file(s) in {:.1}s",
            project_name,
            schema_count,
            loaded_files.len(),
            project_start.elapsed().as_secs_f64()
        );
        tracing::info!("{}", project_msg);
        client.log_message(MessageType::INFO, &project_msg).await;
    }

    // Publish config file diagnostics (content mismatches)
    if !content_mismatch_errors.is_empty() {
        let config_uri =
            Uri::from_str(&graphql_ide::path_to_file_uri(config_path)).expect("valid config path");
        let config_content = std::fs::read_to_string(config_path).unwrap_or_default();

        let diagnostics =
            validation_errors_to_diagnostics(&content_mismatch_errors, &config_content);

        if !content_mismatch_errors.is_empty() {
            tracing::warn!(
                "Found {} content mismatch error(s) in config",
                content_mismatch_errors.len()
            );
        }

        client
            .publish_diagnostics(config_uri, diagnostics, None)
            .await;
    }

    tracing::debug!(
        "Background loading finished in {:.2}s",
        start.elapsed().as_secs_f64()
    );
}

impl GraphQLLanguageServer {
    pub fn new(client: Client, reload_handle: Option<crate::trace_capture::ReloadHandle>) -> Self {
        let trace_capture =
            reload_handle.map(|h| Arc::new(crate::trace_capture::TraceCaptureManager::new(h)));
        Self {
            client,
            client_capabilities: Arc::new(RwLock::new(None)),
            workspace: Arc::new(WorkspaceManager::new()),
            trace_capture,
        }
    }

    /// Acquire a snapshot and run a query on a blocking thread.
    /// This allows tower-lsp's built-in `$/cancelRequest` to abort the handler
    /// while computation runs, and prevents sync Salsa queries from blocking
    /// the async runtime.
    pub(crate) async fn with_analysis<F, T>(&self, uri: &Uri, f: F) -> Result<Option<T>>
    where
        F: FnOnce(graphql_ide::Analysis, graphql_ide::FilePath) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(uri)
        else {
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let file_path = graphql_ide::FilePath::new(uri.to_string());
        let uri_for_log = uri.to_string();
        let result = tokio::task::spawn_blocking(move || f(analysis, file_path))
            .await
            .map_err(|join_err| {
                let payload = describe_join_error(join_err);
                tracing::error!(uri = %uri_for_log, "Analysis task ended abnormally: {payload}");
                Error::internal_error()
            })?;
        Ok(result)
    }

    /// Run a blocking Salsa computation on a dedicated thread.
    ///
    /// All `Analysis` method calls (diagnostics, completions, goto-definition, …)
    /// perform synchronous Salsa queries that can be expensive — especially on
    /// schema changes in large projects, where recomputation fans out to every
    /// file. Running these inline on the async runtime blocks the event loop and
    /// starves other tasks (health-check pings, cancellation, etc.).
    ///
    /// Use this helper (or [`with_analysis`]) for **every** Salsa call site.
    /// It moves the closure to the blocking thread-pool so the event loop stays
    /// responsive.
    pub(crate) async fn blocking<F, T>(f: F) -> Option<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        match tokio::task::spawn_blocking(f).await {
            Ok(result) => Some(result),
            Err(join_err) => {
                let payload = describe_join_error(join_err);
                tracing::error!("Blocking task ended abnormally: {payload}");
                None
            }
        }
    }

    /// Custom request handler for fetching virtual file content.
    ///
    /// This is used by the editor extension to display virtual files (like introspected
    /// remote schemas) when the user navigates to them via goto definition.
    ///
    /// # Parameters
    /// - `uri`: The URI of the virtual file (e.g., `schema://api.example.com/graphql/schema.graphql`)
    ///
    /// # Returns
    /// The file content as a string, or null if the file is not found.
    #[tracing::instrument(skip(self))]
    pub async fn virtual_file_content(
        &self,
        params: VirtualFileContentParams,
    ) -> Result<Option<String>> {
        tracing::debug!("Virtual file content requested: {}", params.uri);

        let file_path = graphql_ide::FilePath::new(&params.uri);

        // Search all hosts for the file content
        for host in self.workspace.all_hosts() {
            let Some(analysis) = host.try_snapshot().await else {
                continue;
            };

            if let Some(content) = analysis.file_content(&file_path) {
                tracing::debug!("Found virtual file content ({} bytes)", content.len());
                return Ok(Some(content));
            }
        }

        tracing::debug!("Virtual file not found: {}", params.uri);
        Ok(None)
    }

    /// Health check endpoint for the extension to verify the server is responsive.
    ///
    /// Returns a simple response with the current server timestamp. If this request
    /// times out, the extension can assume the server is hung and display a warning.
    #[allow(clippy::unused_async)] // tower-lsp requires async for custom methods
    pub async fn ping(&self) -> Result<PingResponse> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(PingResponse { timestamp })
    }

    #[allow(clippy::unused_async)]
    pub async fn trace_capture(
        &self,
        params: crate::trace_capture::TraceCaptureParams,
    ) -> Result<crate::trace_capture::TraceCaptureResult> {
        let Some(ref manager) = self.trace_capture else {
            return Ok(crate::trace_capture::TraceCaptureResult {
                status: "error".to_string(),
                path: None,
                message: Some("Trace capture not available (tracing not initialized)".to_string()),
                duration_ms: None,
            });
        };

        match params.action.as_str() {
            "start" => Ok(manager.start()),
            "stop" => Ok(manager.stop()),
            _ => Ok(crate::trace_capture::TraceCaptureResult {
                status: "error".to_string(),
                path: None,
                message: Some(format!("Unknown action: {}", params.action)),
                duration_ms: None,
            }),
        }
    }
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    /// Load GraphQL config from a workspace folder and load all project files
    async fn load_workspace_config(&self, workspace_uri: &str, workspace_path: &PathBuf) {
        tracing::debug!(path = ?workspace_path, "Loading GraphQL config");

        self.workspace
            .workspace_roots
            .insert(workspace_uri.to_string(), workspace_path.clone());

        match find_config(workspace_path) {
            Ok(Some(config_path)) => {
                self.workspace
                    .config_paths
                    .insert(workspace_uri.to_string(), config_path.clone());

                match graphql_config::load_config(&config_path) {
                    Ok(config) => {
                        // Validate configuration and always publish diagnostics
                        // (empty list clears previous errors when config becomes valid)
                        let lint_rule_names = graphql_linter::all_rule_names();
                        let lint_context = graphql_config::LintValidationContext {
                            valid_rule_names: &lint_rule_names,
                            valid_presets: &["recommended"],
                        };
                        let errors =
                            graphql_config::validate(&config, workspace_path, Some(&lint_context));
                        let config_uri =
                            Uri::from_str(&graphql_ide::path_to_file_uri(&config_path))
                                .expect("valid config path");

                        let has_errors = errors
                            .iter()
                            .any(|e| e.severity() == graphql_config::Severity::Error);

                        if errors.is_empty() {
                            // Config is valid -- clear any previous validation errors
                            self.client
                                .publish_diagnostics(config_uri, vec![], None)
                                .await;
                        } else {
                            let config_content =
                                std::fs::read_to_string(&config_path).unwrap_or_default();
                            let diagnostics =
                                validation_errors_to_diagnostics(&errors, &config_content);
                            self.client
                                .publish_diagnostics(config_uri.clone(), diagnostics, None)
                                .await;

                            if has_errors {
                                let error_count = errors
                                    .iter()
                                    .filter(|e| e.severity() == graphql_config::Severity::Error)
                                    .count();

                                let actions = vec![
                                    MessageActionItem {
                                        title: "Open Config".to_string(),
                                        properties: HashMap::default(),
                                    },
                                    MessageActionItem {
                                        title: "Dismiss".to_string(),
                                        properties: HashMap::default(),
                                    },
                                ];

                                let response = self
                                    .client
                                    .show_message_request(
                                        MessageType::ERROR,
                                        format!(
                                            "GraphQL config has {error_count} validation error(s). \
                                            Please fix the configuration before continuing.",
                                        ),
                                        Some(actions),
                                    )
                                    .await;

                                if let Ok(Some(action)) = response {
                                    if action.title == "Open Config" {
                                        let _ = self
                                            .client
                                            .show_document(ShowDocumentParams {
                                                uri: config_uri,
                                                external: Some(false),
                                                take_focus: Some(true),
                                                selection: None,
                                            })
                                            .await;
                                    }
                                }

                                return;
                            }
                        }

                        self.client
                            .log_message(
                                MessageType::INFO,
                                "GraphQL config found, loading files...",
                            )
                            .await;

                        self.workspace
                            .configs
                            .insert(workspace_uri.to_string(), config.clone());

                        self.load_all_project_files(
                            workspace_uri,
                            workspace_path,
                            &config,
                            &config_path,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!("Error loading config: {}", e);
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                &format!("Failed to load GraphQL config: {e}"),
                            )
                            .await;
                    }
                }
            }
            Ok(None) => {
                let actions = vec![
                    MessageActionItem {
                        title: "Create Config".to_string(),
                        properties: HashMap::default(),
                    },
                    MessageActionItem {
                        title: "Dismiss".to_string(),
                        properties: HashMap::default(),
                    },
                ];

                let response = self
                    .client
                    .show_message_request(
                        MessageType::WARNING,
                        "No GraphQL config found. Schema validation and full IDE features require a config file.",
                        Some(actions),
                    )
                    .await;

                if let Ok(Some(action)) = response {
                    if action.title == "Create Config" {
                        self.create_default_config(workspace_path).await;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Error searching for config: {}", e);
            }
        }
    }

    /// Create a default GraphQL config file in the workspace root
    async fn create_default_config(&self, workspace_path: &Path) {
        let config_path = workspace_path.join("graphql.config.yaml");

        if config_path.exists() {
            self.client
                .show_message(
                    MessageType::INFO,
                    "Config file already exists at graphql.config.yaml",
                )
                .await;
            return;
        }

        let default_config = r#"# GraphQL configuration
# See: https://the-guild.dev/graphql/config/docs

schema: "schema.graphql"
documents: "**/*.graphql"
"#;

        match std::fs::write(&config_path, default_config) {
            Ok(()) => {
                self.client
                    .show_message(
                        MessageType::INFO,
                        "Created graphql.config.yaml. Update the schema path and reload the window.",
                    )
                    .await;
            }
            Err(e) => {
                self.client
                    .show_message(MessageType::ERROR, format!("Failed to create config: {e}"))
                    .await;
            }
        }
    }

    /// Fetch remote schemas via introspection and add them as virtual files.
    ///
    /// This method fetches GraphQL schemas from remote endpoints using introspection
    /// queries, converts them to SDL, and registers them as virtual schema files.
    /// This enables full IDE features (diagnostics, completions, etc.) for operations
    /// that reference remote schemas.
    #[tracing::instrument(skip(self, host, pending_introspections))]
    async fn fetch_remote_schemas(
        &self,
        host: &ProjectHost,
        pending_introspections: &[graphql_ide::PendingIntrospection],
        project_name: &str,
    ) {
        tracing::debug!(
            "Fetching {} remote schema(s) for project '{}'",
            pending_introspections.len(),
            project_name
        );

        for pending in pending_introspections {
            let url = &pending.url;
            tracing::debug!("Introspecting remote schema: {}", url);

            // Build the introspection client with config options
            let mut client = graphql_introspect::IntrospectionClient::new();

            if let Some(headers) = &pending.headers {
                for (name, value) in headers {
                    client = client.with_header(name, value);
                }
            }

            if let Some(timeout) = pending.timeout {
                client = client.with_timeout(Duration::from_secs(timeout));
            }

            if let Some(retries) = pending.retry {
                client = client.with_retries(retries);
            }

            // Execute the introspection query
            match client.execute(url).await {
                Ok(response) => {
                    // Convert introspection response to SDL
                    let sdl = graphql_introspect::introspection_to_sdl(&response);
                    tracing::debug!(
                        "Successfully introspected schema from {} ({} bytes SDL)",
                        url,
                        sdl.len()
                    );

                    // Add the introspected schema as a virtual file
                    let virtual_uri = host
                        .with_write(|h| h.add_introspected_schema(url, &sdl))
                        .await;

                    tracing::debug!("Registered remote schema as virtual file: {}", virtual_uri);

                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("Loaded remote schema from {url}"),
                        )
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to introspect schema from {}: {}", url, e);
                    self.client
                        .show_message(
                            MessageType::ERROR,
                            format!("Failed to load remote schema from {url}: {e}"),
                        )
                        .await;
                }
            }
        }
    }

    /// Load all GraphQL files from the config into `AnalysisHost`
    async fn load_all_project_files(
        &self,
        workspace_uri: &str,
        workspace_path: &Path,
        config: &graphql_config::GraphQLConfig,
        config_path: &Path,
    ) {
        let start = std::time::Instant::now();
        let projects: Vec<_> = config.projects().collect();
        tracing::debug!("Loading files for {} project(s)", projects.len());

        // Collect all content mismatch errors across all projects
        let mut content_mismatch_errors: Vec<graphql_config::ConfigValidationError> = Vec::new();
        // Track projects and their unmatched schema patterns
        // Each tuple is (project_name, unmatched_patterns, has_no_schema)
        let mut schema_pattern_results: Vec<(String, Vec<String>, bool)> = Vec::new();

        for (project_name, project_config) in projects {
            let project_start = std::time::Instant::now();
            tracing::debug!("Loading project: {}", project_name);
            let extract_config = project_config
                .extensions
                .as_ref()
                .and_then(|extensions| extensions.get("extractConfig"))
                .and_then(|extract_config_value| {
                    match serde_json::from_value::<graphql_extract::ExtractConfig>(
                        extract_config_value.clone(),
                    ) {
                        Ok(config) => Some(config),
                        Err(e) => {
                            tracing::warn!("Failed to parse extract config: {e}, using defaults");
                            None
                        }
                    }
                })
                .unwrap_or_default();

            let lint_config = project_config.lint().map_or_else(
                graphql_linter::LintConfig::default,
                |lint_value| match serde_json::from_value::<graphql_linter::LintConfig>(lint_value.clone()) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        tracing::warn!("Failed to parse lint config for project '{}': {}. Using default lint config.", project_name, e);
                        graphql_linter::LintConfig::default()
                    }
                },
            );

            let host = self
                .workspace
                .get_or_create_host(workspace_uri, project_name);

            host.with_write(|h| {
                h.set_extract_config(extract_config.clone());
                h.set_lint_config(lint_config);
            })
            .await;

            // Load local schemas AND documents in a single lock acquisition to prevent
            // race conditions where did_save could run between schema and document loading,
            // resulting in project-wide lints running with incomplete document_file_ids.
            let (schema_result, loaded_files, _doc_result) = host
                .with_write(|h| {
                    // Load schemas first
                    let schema_result =
                        match h.load_schemas_from_config(project_config, workspace_path) {
                            Ok(result) => {
                                tracing::debug!(
                                    "Loaded {} local schema file(s), {} remote schema(s) pending",
                                    result.loaded_count,
                                    result.pending_introspections.len()
                                );
                                result
                            }
                            Err(e) => {
                                tracing::error!("Failed to load schemas: {}", e);
                                graphql_ide::SchemaLoadResult::default()
                            }
                        };

                    // Load documents in the same lock acquisition
                    let (docs, doc_result) = h.load_documents_from_config(
                        project_config,
                        workspace_path,
                        &extract_config,
                    );

                    (schema_result, docs, doc_result)
                })
                .await;

            let no_user_schema = schema_result.has_no_user_schema();
            let pending_introspections = schema_result.pending_introspections.clone();
            let schema_errors = schema_result.content_errors.clone();
            let unmatched_patterns = schema_result.unmatched_patterns.clone();

            // Track unmatched patterns for diagnostics
            if !unmatched_patterns.is_empty() || no_user_schema {
                schema_pattern_results.push((
                    project_name.to_string(),
                    unmatched_patterns,
                    no_user_schema,
                ));
            }

            if no_user_schema {
                tracing::warn!(
                    "Project '{}': no schema files found matching configured patterns",
                    project_name
                );
                self.client
                    .show_message(
                        MessageType::WARNING,
                        format!("GraphQL: No schema files found for project '{project_name}'. Schema validation will be skipped."),
                    )
                    .await;
            }

            // Convert schema content mismatch errors to ConfigValidationError
            for error in &schema_errors {
                tracing::warn!(
                    "Content mismatch in '{}': file in schema config contains executable definitions: {}",
                    error.file_path.display(),
                    error.unexpected_definitions.join(", ")
                );
                content_mismatch_errors.push(
                    graphql_config::ConfigValidationError::ContentMismatch {
                        project: project_name.to_string(),
                        pattern: error.pattern.clone(),
                        expected: graphql_config::FileType::Schema,
                        file_path: error.file_path.clone(),
                        unexpected_definitions: error.unexpected_definitions.clone(),
                    },
                );
            }

            // Fetch remote schemas via introspection (async, outside lock)
            // This happens after both local schemas and documents are loaded,
            // so project-wide lints will at least see all local files.
            if !pending_introspections.is_empty() {
                self.fetch_remote_schemas(&host, &pending_introspections, project_name)
                    .await;
            }

            if !loaded_files.is_empty() {
                let total_files_loaded = loaded_files.len();
                tracing::debug!(
                    "Collected {} document files for project '{}'",
                    total_files_loaded,
                    project_name
                );

                // Register files in the file-to-project index
                for loaded_file in &loaded_files {
                    self.workspace.file_to_project.insert(
                        loaded_file.path.as_str().to_string(),
                        (workspace_uri.to_string(), project_name.to_string()),
                    );
                }

                tracing::debug!(
                    "Finished loading documents for project '{}': {} files total",
                    project_name,
                    total_files_loaded
                );

                // Note: load_documents_from_config uses add_files_batch internally,
                // which rebuilds the ProjectFiles index automatically

                // Publish initial diagnostics for all loaded files
                tracing::debug!(
                    "Publishing initial diagnostics for {} files...",
                    total_files_loaded
                );
                let diag_start = std::time::Instant::now();

                let Some(snapshot) = host.try_snapshot().await else {
                    tracing::warn!("Could not get snapshot for initial diagnostics");
                    continue;
                };

                // Get file paths from loaded files
                let loaded_file_paths: Vec<graphql_ide::FilePath> =
                    loaded_files.iter().map(|f| f.path.clone()).collect();

                // Use the new all_diagnostics_for_files helper to merge per-file and project-wide diagnostics
                let Some(all_diagnostics_map) = GraphQLLanguageServer::blocking(move || {
                    snapshot.all_diagnostics_for_files(&loaded_file_paths)
                })
                .await
                else {
                    tracing::error!(
                        "Diagnostics computation panicked during workspace config load"
                    );
                    continue;
                };

                tracing::debug!(
                    "Publishing diagnostics for {} files with issues",
                    all_diagnostics_map.len()
                );

                for (file_path, diagnostics) in &all_diagnostics_map {
                    let Ok(file_uri) = Uri::from_str(file_path.as_str()) else {
                        continue;
                    };

                    let lsp_diagnostics: Vec<Diagnostic> = diagnostics
                        .iter()
                        .cloned()
                        .map(convert_ide_diagnostic)
                        .collect();

                    self.client
                        .publish_diagnostics(file_uri, lsp_diagnostics, None)
                        .await;
                }

                // Also publish empty diagnostics for files with no issues
                // (this clears stale diagnostics from previous sessions)
                for loaded_file in &loaded_files {
                    if !all_diagnostics_map.contains_key(&loaded_file.path) {
                        if let Ok(file_uri) = Uri::from_str(loaded_file.path.as_str()) {
                            self.client
                                .publish_diagnostics(file_uri, vec![], None)
                                .await;
                        }
                    }
                }

                tracing::debug!(
                    "Initial diagnostics published in {:.2}s",
                    diag_start.elapsed().as_secs_f64()
                );
            }

            let project_msg = format!(
                "Project '{}' loaded: {} schema file(s), {} document file(s) in {:.1}s",
                project_name,
                schema_result.loaded_count,
                loaded_files.len(),
                project_start.elapsed().as_secs_f64()
            );
            tracing::info!("{}", project_msg);
            self.client
                .log_message(MessageType::INFO, &project_msg)
                .await;
        }

        // Publish config file diagnostics (content mismatches)
        if !content_mismatch_errors.is_empty() {
            let config_uri = Uri::from_str(&graphql_ide::path_to_file_uri(config_path))
                .expect("valid config path");
            let config_content = std::fs::read_to_string(config_path).unwrap_or_default();

            let diagnostics =
                validation_errors_to_diagnostics(&content_mismatch_errors, &config_content);

            tracing::warn!(
                "Found {} content mismatch error(s) in config",
                content_mismatch_errors.len()
            );

            self.client
                .publish_diagnostics(config_uri, diagnostics, None)
                .await;
        }

        let elapsed = start.elapsed();
        let init_message = format!(
            "Project initialization complete: {} files loaded in {:.1}s",
            self.workspace.file_to_project.len(),
            elapsed.as_secs_f64()
        );
        tracing::info!("{}", init_message);
        self.client
            .log_message(MessageType::INFO, &init_message)
            .await;

        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") || line.starts_with("VmSize:") {
                        tracing::debug!("Memory: {}", line.trim());
                    }
                }
            }
        }
    }

    /// Reload configuration for a workspace
    ///
    /// This clears all existing hosts for the workspace and reloads the config
    /// from disk, then re-discovers and loads all project files.
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    pub(crate) async fn reload_workspace_config(&self, workspace_uri: &str) {
        tracing::debug!("Reloading configuration for workspace: {}", workspace_uri);

        let Some(workspace_path) = self
            .workspace
            .workspace_roots
            .get(workspace_uri)
            .map(|r| r.clone())
        else {
            tracing::error!(
                "Cannot reload config: workspace root not found for {}",
                workspace_uri
            );
            return;
        };

        tracing::debug!("Clearing hosts and file mappings for workspace");
        self.workspace.clear_workspace(workspace_uri);

        self.workspace.configs.remove(workspace_uri);
        self.load_workspace_config(workspace_uri, &workspace_path)
            .await;

        // Only show success if the config was actually loaded
        // (load_workspace_config stores the config only when validation passes)
        if self.workspace.configs.contains_key(workspace_uri) {
            self.client
                .show_message(
                    MessageType::INFO,
                    "GraphQL configuration reloaded successfully",
                )
                .await;
        }

        tracing::debug!(
            "Configuration reload complete for workspace: {}",
            workspace_uri
        );
    }
}

/// Convert a `tokio::task::JoinError` into a printable description that
/// includes the actual panic payload (string or `&'static str`) for panic
/// errors. The default `Display` impl on `JoinError` only says "task N
/// panicked", which is useless for diagnosing what actually went wrong —
/// the panic location and any backtrace also need to be captured by the
/// panic hook (see `install_panic_hook` in `lib.rs`).
///
/// Consumes the error because `into_panic()` requires ownership.
pub(crate) fn describe_join_error(join_err: tokio::task::JoinError) -> String {
    if join_err.is_cancelled() {
        return "cancelled".to_string();
    }
    if !join_err.is_panic() {
        return join_err.to_string();
    }
    // is_panic() guarantees into_panic() returns the payload.
    let payload: Box<dyn std::any::Any + Send> = join_err.into_panic();
    let msg = if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else {
        "<non-string panic payload>".to_string()
    };
    format!("panic: {msg}")
}

impl LanguageServer for GraphQLLanguageServer {
    #[tracing::instrument(skip(self, params))]
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing GraphQL Language Server");

        {
            let mut caps = self.client_capabilities.write().await;
            *caps = Some(params.capabilities.clone());
        }

        let text_document_caps = params.capabilities.text_document.as_ref();

        let supports_incremental_sync = text_document_caps
            .and_then(|td| td.synchronization.as_ref())
            .is_some();
        tracing::debug!(
            supports_incremental_sync,
            "Client text document sync capability"
        );

        let supports_hover = text_document_caps
            .and_then(|td| td.hover.as_ref())
            .is_some();

        let supports_completion = text_document_caps
            .and_then(|td| td.completion.as_ref())
            .is_some();

        let supports_definition = text_document_caps
            .and_then(|td| td.definition.as_ref())
            .is_some();

        let supports_references = text_document_caps
            .and_then(|td| td.references.as_ref())
            .is_some();

        let supports_document_symbols = text_document_caps
            .and_then(|td| td.document_symbol.as_ref())
            .is_some();

        let workspace_caps = params.capabilities.workspace.as_ref();
        let supports_workspace_symbols = workspace_caps.and_then(|ws| ws.symbol.as_ref()).is_some();

        // Check if client supports semantic tokens
        let supports_semantic_tokens = text_document_caps
            .and_then(|td| td.semantic_tokens.as_ref())
            .is_some();

        // Check if client supports code lens
        let supports_code_lens = text_document_caps
            .and_then(|td| td.code_lens.as_ref())
            .is_some();

        // Check if client supports folding ranges
        let supports_folding_range = text_document_caps
            .and_then(|td| td.folding_range.as_ref())
            .is_some();

        // Check if client supports inlay hints
        let supports_inlay_hints = text_document_caps
            .and_then(|td| td.inlay_hint.as_ref())
            .is_some();

        // Check if client supports selection range
        let supports_selection_range = text_document_caps
            .and_then(|td| td.selection_range.as_ref())
            .is_some();

        let supports_rename = text_document_caps
            .and_then(|td| td.rename.as_ref())
            .is_some();

        let supports_signature_help = text_document_caps
            .and_then(|td| td.signature_help.as_ref())
            .is_some();

        tracing::debug!(
            supports_hover,
            supports_completion,
            supports_definition,
            supports_references,
            supports_document_symbols,
            supports_workspace_symbols,
            supports_semantic_tokens,
            supports_code_lens,
            supports_folding_range,
            supports_inlay_hints,
            supports_selection_range,
            supports_rename,
            supports_signature_help,
            "Client capabilities detected"
        );

        if let Some(ref folders) = params.workspace_folders {
            tracing::debug!(count = folders.len(), "Workspace folders received");
            for folder in folders {
                tracing::debug!(
                    "Workspace folder: name={}, uri={}",
                    folder.name,
                    folder.uri.as_str()
                );
                if let Some(path) = folder.uri.to_file_path() {
                    tracing::debug!("  -> Path: {}", path.display());
                    self.workspace
                        .init_workspace_folders
                        .insert(folder.uri.to_string(), path.into_owned());
                } else {
                    tracing::warn!("  -> Could not convert URI to file path");
                }
            }
        } else {
            tracing::warn!("No workspace folders provided in initialization");
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: supports_completion.then(|| CompletionOptions {
                    trigger_characters: Some(vec![
                        "{".to_string(),
                        "@".to_string(),
                        "(".to_string(),
                        "$".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: supports_hover.then_some(HoverProviderCapability::Simple(true)),
                signature_help_provider: supports_signature_help.then(|| SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                definition_provider: supports_definition.then_some(OneOf::Left(true)),
                references_provider: supports_references.then_some(OneOf::Left(true)),
                document_symbol_provider: supports_document_symbols.then_some(OneOf::Left(true)),
                workspace_symbol_provider: supports_workspace_symbols.then_some(OneOf::Left(true)),
                code_action_provider: Some(lsp_types::CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        resolve_provider: None,
                    },
                )),
                semantic_tokens_provider: supports_semantic_tokens.then(|| {
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: vec![
                                SemanticTokenType::TYPE,
                                SemanticTokenType::PROPERTY,
                                SemanticTokenType::VARIABLE,
                                SemanticTokenType::FUNCTION,
                                SemanticTokenType::ENUM_MEMBER,
                                SemanticTokenType::KEYWORD,
                                SemanticTokenType::STRING,
                                SemanticTokenType::NUMBER,
                            ],
                            token_modifiers: vec![
                                SemanticTokenModifier::DEPRECATED,
                                SemanticTokenModifier::DEFINITION,
                            ],
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: None,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    })
                }),
                code_lens_provider: supports_code_lens.then_some(CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                folding_range_provider: supports_folding_range
                    .then_some(FoldingRangeProviderCapability::Simple(true)),
                inlay_hint_provider: supports_inlay_hints.then_some(OneOf::Right(
                    InlayHintServerCapabilities::Options(InlayHintOptions {
                        resolve_provider: Some(false),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    }),
                )),
                selection_range_provider: supports_selection_range
                    .then_some(SelectionRangeProviderCapability::Simple(true)),
                rename_provider: supports_rename.then_some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["graphql-analyzer.checkStatus".to_string()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "GraphQL Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        let version = env!("CARGO_PKG_VERSION");
        let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
        let git_dirty = option_env!("VERGEN_GIT_DIRTY").unwrap_or("false");
        let build_timestamp = option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("unknown");
        let binary_path = std::env::current_exe()
            .map_or_else(|_| "unknown".to_string(), |p| p.display().to_string());

        let dirty_suffix = if git_dirty == "true" { "-dirty" } else { "" };

        tracing::info!(
            version = version,
            git_sha = format!("{git_sha}{dirty_suffix}"),
            build_timestamp = build_timestamp,
            binary_path = binary_path,
            "GraphQL Language Server initialized"
        );

        self.client
            .log_message(
                MessageType::INFO,
                format!("GraphQL LSP initialized (v{version} @ {git_sha}{dirty_suffix}, built {build_timestamp}, binary: {binary_path})"),
            )
            .await;

        // Spawn background task for heavy initialization work
        // This allows the LSP to respond to requests while loading
        let folders: Vec<(String, PathBuf)> = self
            .workspace
            .init_workspace_folders
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        if folders.is_empty() {
            tracing::debug!("No workspace folders to load");
            self.client
                .send_notification::<StatusNotification>(StatusParams {
                    status: "ready".to_string(),
                    message: Some("No workspace folders".to_string()),
                })
                .await;
            return;
        }

        // Clone what we need for the background task
        let client = self.client.clone();
        let workspace = Arc::clone(&self.workspace);

        tokio::spawn(async move {
            load_workspaces_background(client, workspace, folders).await;
        });
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down GraphQL Language Server");
        Ok(())
    }

    #[tracing::instrument(skip(self, params), fields(path = %params.text_document.uri.as_str()))]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        handlers::document_sync::handle_did_open(self, params).await;
    }

    #[tracing::instrument(skip(self, params), fields(path = %params.text_document.uri.as_str()))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        handlers::document_sync::handle_did_change(self, params).await;
    }

    #[tracing::instrument(skip(self, params), fields(path = %params.text_document.uri.as_str()))]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        handlers::document_sync::handle_did_save(self, params).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        handlers::document_sync::handle_did_close(self, params).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        handlers::document_sync::handle_did_change_watched_files(self, params).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        handlers::editing::handle_completion(self, params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        handlers::display::handle_hover(self, params).await
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<lsp_types::SignatureHelp>> {
        handlers::editing::handle_signature_help(self, params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        handlers::navigation::handle_goto_definition(self, params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        handlers::navigation::handle_references(self, params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        handlers::editing::handle_prepare_rename(self, params).await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        handlers::editing::handle_rename(self, params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        handlers::navigation::handle_document_symbol(self, params).await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<lsp_types::WorkspaceSymbolResponse>> {
        handlers::navigation::handle_workspace_symbol(self, params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        handlers::display::handle_semantic_tokens_full(self, params).await
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        handlers::display::handle_selection_range(self, params).await
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        handlers::editing::handle_execute_command(self, params).await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        handlers::editing::handle_code_action(self, params).await
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        handlers::display::handle_code_lens(self, params).await
    }

    async fn code_lens_resolve(&self, code_lens: CodeLens) -> Result<CodeLens> {
        handlers::display::handle_code_lens_resolve(code_lens).await
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        handlers::display::handle_folding_range(self, params).await
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<LspInlayHint>>> {
        handlers::display::handle_inlay_hint(self, params).await
    }
}
