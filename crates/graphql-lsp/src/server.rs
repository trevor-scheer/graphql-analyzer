use crate::conversions::{
    convert_ide_code_lens, convert_ide_code_lens_info, convert_ide_completion_item,
    convert_ide_diagnostic, convert_ide_document_symbol, convert_ide_hover, convert_ide_location,
    convert_ide_workspace_symbol, convert_lsp_position,
};
use crate::workspace::{ProjectHost, WorkspaceManager};
use graphql_config::find_config;
use lsp_types::{
    ClientCapabilities, CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand,
    CodeActionParams, CodeActionResponse, CodeLens, CodeLensParams, CompletionOptions,
    CompletionParams, CompletionResponse, Diagnostic, DidChangeTextDocumentParams,
    DidChangeWatchedFilesParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, ExecuteCommandOptions,
    ExecuteCommandParams, FileChangeType, FileSystemWatcher, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, Location, MessageActionItem, MessageType, OneOf,
    ReferenceParams, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    SymbolInformation, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
    WorkDoneProgressOptions, WorkspaceEdit, WorkspaceSymbol, WorkspaceSymbolParams,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::{Client, LanguageServer, UriExt};

/// Parameters for the `graphql/virtualFileContent` custom request.
///
/// This request fetches the content of virtual files (like introspected remote schemas)
/// that don't exist on disk but are registered in the LSP's file registry.
#[derive(Debug, serde::Deserialize)]
pub struct VirtualFileContentParams {
    /// The URI of the virtual file to fetch (e.g., `schema://api.example.com/graphql/schema.graphql`)
    pub uri: String,
}

pub struct GraphQLLanguageServer {
    client: Client,
    /// Client capabilities received during initialization
    client_capabilities: Arc<RwLock<Option<ClientCapabilities>>>,
    /// Workspace manager for all workspace/project state
    workspace: Arc<WorkspaceManager>,
}

/// Background task that loads workspace configs and publishes initial diagnostics.
/// Runs asynchronously so the LSP can respond to requests during loading.
async fn load_workspaces_background(
    client: Client,
    workspace: Arc<WorkspaceManager>,
    folders: Vec<(String, PathBuf)>,
) {
    tracing::info!(
        "Loading configs for {} workspace(s) in background",
        folders.len()
    );

    for (uri, path) in folders {
        tracing::info!(
            "Loading config for workspace: {} at {}",
            uri,
            path.display()
        );
        load_workspace_config_background(&client, &workspace, &uri, &path).await;
    }

    tracing::info!(
        "Background loading complete: {} workspace roots, {} configs",
        workspace.workspace_roots.len(),
        workspace.configs.len()
    );

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

    tracing::info!(
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
                .unwrap(),
        ),
    };

    if let Err(e) = client.register_capability(vec![registration]).await {
        tracing::error!("Failed to register config file watchers: {:?}", e);
    }
}

/// Load a single workspace config in the background
#[allow(clippy::too_many_lines)]
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
            tracing::info!("No GraphQL config found in workspace");
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
#[allow(clippy::too_many_lines)]
async fn load_all_project_files_background(
    client: &Client,
    workspace: &Arc<WorkspaceManager>,
    workspace_uri: &str,
    workspace_path: &Path,
    config: &graphql_config::GraphQLConfig,
) {
    const MAX_FILES_WARNING_THRESHOLD: usize = 1000;
    let start = std::time::Instant::now();
    let projects: Vec<_> = config.projects().collect();
    tracing::info!(
        "Loading files for {} project(s) in background",
        projects.len()
    );

    for (project_name, project_config) in projects {
        tracing::info!("Loading project: {}", project_name);

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

        let lint_config = project_config.lint.as_ref().map_or_else(
            graphql_linter::LintConfig::default,
            |lint_value| {
                serde_json::from_value::<graphql_linter::LintConfig>(lint_value.clone())
                    .unwrap_or_default()
            },
        );

        let host = workspace.get_or_create_host(workspace_uri, project_name);

        host.with_write(|h| {
            h.set_extract_config(extract_config.clone());
            h.set_lint_config(lint_config);
        })
        .await;

        // Load schemas
        let pending_introspections = host
            .with_write(
                |h| match h.load_schemas_from_config(project_config, workspace_path) {
                    Ok(result) => {
                        tracing::info!(
                            "Loaded {} local schema file(s), {} remote pending",
                            result.loaded_count,
                            result.pending_introspections.len()
                        );
                        result.pending_introspections
                    }
                    Err(e) => {
                        tracing::error!("Failed to load schemas: {}", e);
                        vec![]
                    }
                },
            )
            .await;

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
        let discovered_files = graphql_ide::discover_document_files(project_config, workspace_path);

        if discovered_files.is_empty() {
            continue;
        }

        let total_files = discovered_files.len();
        tracing::info!(
            "Discovered {} document files for project '{}'",
            total_files,
            project_name
        );

        if total_files >= MAX_FILES_WARNING_THRESHOLD {
            client
                .show_message(
                    MessageType::WARNING,
                    format!("GraphQL LSP: Loading {total_files} files, this may take a while."),
                )
                .await;
        }

        // Phase 2: Register files (brief lock acquisition)
        let loaded_files = host
            .with_write(|h| h.add_discovered_files(&discovered_files))
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

        // Pre-warm expensive caches in background so first file open is fast
        // This builds fragment_spreads_index, merged_schema, and all_fragments
        tracing::info!("Pre-warming caches for project '{}'...", project_name);
        let warm_start = std::time::Instant::now();
        if let Some(snapshot) = host.try_snapshot().await {
            snapshot.warm_caches();
            tracing::info!(
                "Cache warming completed in {:.2}s",
                warm_start.elapsed().as_secs_f64()
            );
        }

        // NOTE: We intentionally skip computing/publishing diagnostics during background loading.
        // Computing diagnostics for 10k+ files causes performance issues and potential OOM.
        // Users get diagnostics when they open files via didOpen/didChange handlers.
    }

    tracing::info!(
        "Background loading finished in {:.2}s",
        start.elapsed().as_secs_f64()
    );
}

impl GraphQLLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            client_capabilities: Arc::new(RwLock::new(None)),
            workspace: Arc::new(WorkspaceManager::new()),
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
        for entry in &self.workspace.hosts {
            let host = entry.value();
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

    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    /// Load GraphQL config from a workspace folder and load all project files
    async fn load_workspace_config(&self, workspace_uri: &str, workspace_path: &PathBuf) {
        tracing::info!(path = ?workspace_path, "Loading GraphQL config");

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
                        self.client
                            .log_message(
                                MessageType::INFO,
                                "GraphQL config found, loading files...",
                            )
                            .await;

                        self.workspace
                            .configs
                            .insert(workspace_uri.to_string(), config.clone());

                        self.load_all_project_files(workspace_uri, workspace_path, &config)
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
        tracing::info!(
            "Fetching {} remote schema(s) for project '{}'",
            pending_introspections.len(),
            project_name
        );

        for pending in pending_introspections {
            let url = &pending.url;
            tracing::info!("Introspecting remote schema: {}", url);

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
                    tracing::info!(
                        "Successfully introspected schema from {} ({} bytes SDL)",
                        url,
                        sdl.len()
                    );

                    // Add the introspected schema as a virtual file
                    let virtual_uri = host
                        .with_write(|h| h.add_introspected_schema(url, &sdl))
                        .await;

                    tracing::info!("Registered remote schema as virtual file: {}", virtual_uri);

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
    #[allow(clippy::too_many_lines)]
    async fn load_all_project_files(
        &self,
        workspace_uri: &str,
        workspace_path: &Path,
        config: &graphql_config::GraphQLConfig,
    ) {
        const MAX_FILES_WARNING_THRESHOLD: usize = 1000;
        let start = std::time::Instant::now();
        let projects: Vec<_> = config.projects().collect();
        tracing::info!("Loading files for {} project(s)", projects.len());

        for (project_name, project_config) in projects {
            tracing::info!("Loading project: {}", project_name);
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

            let lint_config = project_config.lint.as_ref().map_or_else(
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

            // Load local schemas and collect pending remote introspections
            let pending_introspections = host
                .with_write(
                    |h| match h.load_schemas_from_config(project_config, workspace_path) {
                        Ok(result) => {
                            tracing::info!(
                                "Loaded {} local schema file(s), {} remote schema(s) pending",
                                result.loaded_count,
                                result.pending_introspections.len()
                            );
                            result.pending_introspections
                        }
                        Err(e) => {
                            tracing::error!("Failed to load schemas: {}", e);
                            vec![]
                        }
                    },
                )
                .await;

            // Fetch remote schemas via introspection (async)
            if !pending_introspections.is_empty() {
                self.fetch_remote_schemas(&host, &pending_introspections, project_name)
                    .await;
            }

            // Use load_documents_from_config from graphql-ide to handle file discovery
            let loaded_files = host
                .with_write(|h| h.load_documents_from_config(project_config, workspace_path))
                .await;

            if !loaded_files.is_empty() {
                let total_files_loaded = loaded_files.len();
                tracing::info!(
                    "Collected {} document files for project '{}'",
                    total_files_loaded,
                    project_name
                );

                // Show warning for large file counts
                if total_files_loaded >= MAX_FILES_WARNING_THRESHOLD {
                    tracing::warn!(
                        "Loading large number of files ({}), this may take a while...",
                        total_files_loaded
                    );
                    self.client
                        .show_message(
                            MessageType::WARNING,
                            format!(
                                "GraphQL LSP: Loading {total_files_loaded} files, this may take a while. \
                                Consider using more specific patterns if this is too slow."
                            ),
                        )
                        .await;
                }

                // Register files in the file-to-project index
                for loaded_file in &loaded_files {
                    self.workspace.file_to_project.insert(
                        loaded_file.path.as_str().to_string(),
                        (workspace_uri.to_string(), project_name.to_string()),
                    );
                }

                tracing::info!(
                    "Finished loading documents for project '{}': {} files total",
                    project_name,
                    total_files_loaded
                );

                // Note: load_documents_from_config uses add_files_batch internally,
                // which rebuilds the ProjectFiles index automatically

                // NOTE: We intentionally skip computing/publishing diagnostics during loading.
                // Computing diagnostics for 10k+ files causes performance issues and potential OOM.
                // Users get diagnostics when they open files via didOpen/didChange handlers.
            }
        }

        let elapsed = start.elapsed();
        tracing::info!(
            "Finished loading all project files into AnalysisHost in {:.2}s",
            elapsed.as_secs_f64()
        );

        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") || line.starts_with("VmSize:") {
                        tracing::info!("Memory: {}", line.trim());
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
    async fn reload_workspace_config(&self, workspace_uri: &str) {
        tracing::info!("Reloading configuration for workspace: {}", workspace_uri);

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

        let keys_to_remove: Vec<_> = self
            .workspace
            .hosts
            .iter()
            .filter(|entry| entry.key().0 == workspace_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for key in &keys_to_remove {
            tracing::debug!("Removing host for project: {}", key.1);
            self.workspace.hosts.remove(key);
        }

        tracing::info!(
            "Cleared {} existing host(s) for workspace",
            keys_to_remove.len()
        );

        let file_keys_to_remove: Vec<_> = self
            .workspace
            .file_to_project
            .iter()
            .filter(|entry| entry.value().0 == workspace_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for key in &file_keys_to_remove {
            self.workspace.file_to_project.remove(key);
        }

        tracing::info!(
            "Cleared {} file-to-project mappings for workspace",
            file_keys_to_remove.len()
        );

        self.workspace.configs.remove(workspace_uri);
        self.load_workspace_config(workspace_uri, &workspace_path)
            .await;

        self.client
            .show_message(
                MessageType::INFO,
                "GraphQL configuration reloaded successfully",
            )
            .await;

        tracing::info!(
            "Configuration reload complete for workspace: {}",
            workspace_uri
        );
    }

    /// Validate a file and publish diagnostics
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(path = ?uri.to_file_path().unwrap()))]
    async fn validate_file(&self, uri: Uri) {
        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            tracing::warn!("No workspace/project found for file");
            return;
        };

        let Some(host) = self
            .workspace
            .hosts
            .get(&(workspace_uri.clone(), project_name.clone()))
        else {
            tracing::warn!("No analysis host found for workspace/project");
            return;
        };

        let Some(snapshot) = host.try_snapshot().await else {
            tracing::debug!("Could not acquire snapshot for validation");
            return;
        };
        let file_path = graphql_ide::FilePath::new(uri.as_str());
        let diagnostics = snapshot.diagnostics(&file_path);

        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();

        self.client
            .publish_diagnostics(uri, lsp_diagnostics, None)
            .await;
    }

    /// Validate a file using a pre-acquired snapshot
    ///
    /// This variant avoids acquiring the host lock again when we already have a snapshot.
    /// Used by `did_change` after updating a file to avoid double-locking.
    #[tracing::instrument(skip(self, snapshot), fields(path = ?uri.to_file_path().unwrap()))]
    async fn validate_file_with_snapshot(&self, uri: &Uri, snapshot: graphql_ide::Analysis) {
        let file_path = graphql_ide::FilePath::new(uri.as_str());
        let diagnostics = snapshot.diagnostics(&file_path);

        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();

        self.client
            .publish_diagnostics(uri.clone(), lsp_diagnostics, None)
            .await;
    }
}

impl LanguageServer for GraphQLLanguageServer {
    #[allow(clippy::too_many_lines)]
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

        tracing::info!(
            supports_hover,
            supports_completion,
            supports_definition,
            supports_references,
            supports_document_symbols,
            supports_workspace_symbols,
            supports_semantic_tokens,
            supports_code_lens,
            "Client capabilities detected"
        );

        if let Some(ref folders) = params.workspace_folders {
            tracing::info!(count = folders.len(), "Workspace folders received");
            for folder in folders {
                tracing::info!(
                    "Workspace folder: name={}, uri={}",
                    folder.name,
                    folder.uri.as_str()
                );
                if let Some(path) = folder.uri.to_file_path() {
                    tracing::info!("  -> Path: {}", path.display());
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
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: supports_completion.then(|| CompletionOptions {
                    trigger_characters: Some(vec!["{".to_string(), "@".to_string()]),
                    ..Default::default()
                }),
                hover_provider: supports_hover.then_some(HoverProviderCapability::Simple(true)),
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
                // TODO: Code lenses disabled - O(N×M) complexity causes performance issues
                // on large codebases. Need to make fragment_usages() incremental.
                code_lens_provider: None,
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["graphql.checkStatus".to_string()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "GraphQL Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        let version = env!("CARGO_PKG_VERSION");
        let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
        let git_dirty = option_env!("VERGEN_GIT_DIRTY").unwrap_or("false");
        let binary_path = std::env::current_exe()
            .map_or_else(|_| "unknown".to_string(), |p| p.display().to_string());

        let dirty_suffix = if git_dirty == "true" { "-dirty" } else { "" };

        tracing::info!(
            version = version,
            git_sha = format!("{git_sha}{dirty_suffix}"),
            binary_path = binary_path,
            "GraphQL Language Server initialized"
        );

        self.client
            .log_message(
                MessageType::INFO,
                format!("GraphQL LSP initialized (v{version} @ {git_sha}{dirty_suffix}"),
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

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;

        self.workspace
            .document_versions
            .insert(uri.to_string(), version);

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            self.validate_file(uri).await;
            return;
        };

        self.workspace.file_to_project.insert(
            uri.to_string(),
            (workspace_uri.clone(), project_name.clone()),
        );

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);

        let file_kind =
            graphql_syntax::determine_file_kind_from_content(uri.path().as_str(), &content);

        // For TS/JS files, store the original source and let the parsing layer handle extraction.
        // This preserves block boundaries and allows proper validation of separate documents.
        let final_content = content;
        let final_kind = file_kind;

        // Update file and get snapshot in one lock (optimized path using write_and_snapshot)
        let file_path = graphql_ide::FilePath::new(uri.to_string());
        let (_is_new, snapshot) = host
            .write_and_snapshot(|h| h.add_file(&file_path, &final_content, final_kind))
            .await;

        // Validate using pre-acquired snapshot (no lock needed)
        self.validate_file_with_snapshot(&uri, snapshot).await;
    }

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        let uri_string = uri.to_string();
        if let Some(current_version) = self.workspace.document_versions.get(&uri_string) {
            if version <= *current_version {
                tracing::warn!(
                    "Ignoring stale document update: version {} <= current {}",
                    version,
                    *current_version
                );
                return;
            }
        }
        self.workspace.document_versions.insert(uri_string, version);

        for change in params.content_changes {
            let Some((workspace_uri, project_name)) =
                self.workspace.find_workspace_and_project(&uri)
            else {
                continue;
            };

            let host = self
                .workspace
                .get_or_create_host(&workspace_uri, &project_name);

            let file_kind =
                graphql_syntax::determine_file_kind_from_content(uri.path().as_str(), &change.text);

            // For TS/JS files, store the original source and let the parsing layer handle extraction.
            // This preserves block boundaries and allows proper validation of separate documents.
            let final_content = change.text.clone();
            let final_kind = file_kind;

            // Update file and get snapshot in one lock (optimized path using write_and_snapshot)
            let file_path = graphql_ide::FilePath::new(uri.to_string());
            let (_is_new, snapshot) = host
                .write_and_snapshot(|h| h.add_file(&file_path, &final_content, final_kind))
                .await;

            // Validate using pre-acquired snapshot (no lock needed)
            self.validate_file_with_snapshot(&uri, snapshot).await;
        }
    }

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;

        // Find the workspace and project for this file
        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            tracing::debug!(
                "No workspace/project found for saved file, skipping project-wide lints"
            );
            return;
        };

        // Get the analysis host for this workspace/project
        let Some(host) = self
            .workspace
            .hosts
            .get(&(workspace_uri.clone(), project_name.clone()))
        else {
            tracing::debug!("No analysis host found for workspace/project");
            return;
        };

        // Run project-wide lints on save (these are expensive, so we don't run them on every change)
        let Some(snapshot) = host.try_snapshot().await else {
            tracing::debug!("Could not acquire snapshot for project-wide lints");
            return;
        };
        let project_diagnostics = snapshot.project_lint_diagnostics();

        tracing::debug!(
            "Running project-wide lints on save, found diagnostics for {} files",
            project_diagnostics.len()
        );

        // Publish project-wide diagnostics for each affected file
        for (file_path, diagnostics) in project_diagnostics {
            // file_path.as_str() is already a URI string (e.g., "file:///path/to/file.tsx")
            let Ok(file_uri) = Uri::from_str(file_path.as_str()) else {
                tracing::warn!("Invalid URI in project diagnostics: {}", file_path.as_str());
                continue;
            };

            // Get existing per-file diagnostics and merge with project-wide diagnostics
            let per_file_diagnostics = snapshot.diagnostics(&file_path);
            let mut all_diagnostics: Vec<Diagnostic> = per_file_diagnostics
                .into_iter()
                .map(convert_ide_diagnostic)
                .collect();

            // Add project-wide diagnostics
            all_diagnostics.extend(diagnostics.into_iter().map(convert_ide_diagnostic));

            self.client
                .publish_diagnostics(file_uri, all_diagnostics, None)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // NOTE: We intentionally do NOT remove the file from AnalysisHost when it's closed.
        // The file is still part of the project on disk, and other files may reference
        // fragments/types defined in it. Only files that are deleted from disk should be
        // removed from the analysis.

        // Remove version tracking for closed document
        self.workspace
            .document_versions
            .remove(&params.text_document.uri.to_string());

        // Clear diagnostics for the closed file
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        tracing::debug!("Watched files changed: {} file(s)", params.changes.len());

        for change in params.changes {
            let uri = change.uri;
            tracing::debug!("File changed: {:?} (type: {:?})", uri, change.typ);

            let Some(config_path) = uri.to_file_path() else {
                tracing::warn!("Failed to convert URI to file path: {:?}", uri);
                continue;
            };

            let workspace_uri: Option<String> = self
                .workspace
                .config_paths
                .iter()
                .find(|entry| entry.value() == &config_path)
                .map(|entry| entry.key().clone());

            if let Some(workspace_uri) = workspace_uri {
                match change.typ {
                    FileChangeType::CREATED | FileChangeType::CHANGED => {
                        tracing::info!("Config file changed for workspace: {}", workspace_uri);
                        self.reload_workspace_config(&workspace_uri).await;
                    }
                    FileChangeType::DELETED => {
                        tracing::warn!("Config file deleted for workspace: {}", workspace_uri);
                        self.client
                            .show_message(MessageType::WARNING, "GraphQL config file was deleted")
                            .await;
                    }
                    _ => {}
                }
            } else {
                tracing::debug!(
                    "Changed file is not a tracked config file: {:?}",
                    config_path
                );
            }
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let lsp_position = params.text_document_position.position;

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        let Some(items) = analysis.completions(&file_path, position) else {
            return Ok(None);
        };

        let lsp_items: Vec<lsp_types::CompletionItem> =
            items.into_iter().map(convert_ide_completion_item).collect();

        Ok(Some(CompletionResponse::Array(lsp_items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let lsp_position = params.text_document_position_params.position;

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        let Some(hover_result) = analysis.hover(&file_path, position) else {
            return Ok(None);
        };

        let hover = convert_ide_hover(hover_result);

        Ok(Some(hover))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let lsp_position = params.text_document_position_params.position;

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        let Some(locations) = analysis.goto_definition(&file_path, position) else {
            return Ok(None);
        };

        let lsp_locations: Vec<Location> = locations.iter().map(convert_ide_location).collect();

        if lsp_locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(lsp_locations)))
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let lsp_position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        let Some(locations) = analysis.find_references(&file_path, position, include_declaration)
        else {
            return Ok(None);
        };

        let lsp_locations: Vec<Location> = locations
            .into_iter()
            .map(|loc| convert_ide_location(&loc))
            .collect();

        if lsp_locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lsp_locations))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        tracing::debug!("Document symbols requested: {:?}", uri);

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let file_path = graphql_ide::FilePath::new(uri.to_string());

        let symbols = analysis.document_symbols(&file_path);

        if symbols.is_empty() {
            tracing::debug!("No symbols found in document");
            return Ok(None);
        }

        let lsp_symbols: Vec<lsp_types::DocumentSymbol> = symbols
            .into_iter()
            .map(convert_ide_document_symbol)
            .collect();

        tracing::debug!("Returning {} document symbols", lsp_symbols.len());
        Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<OneOf<Vec<SymbolInformation>, Vec<WorkspaceSymbol>>>> {
        tracing::debug!("Workspace symbols requested: {}", params.query);

        let mut all_symbols = Vec::new();

        for entry in &self.workspace.hosts {
            let host = entry.value();
            let Some(analysis) = host.try_snapshot().await else {
                // Skip this host if we can't acquire the lock in time
                continue;
            };

            let symbols = analysis.workspace_symbols(&params.query);
            for symbol in symbols {
                all_symbols.push(convert_ide_workspace_symbol(symbol));
            }
        }

        if all_symbols.is_empty() {
            tracing::debug!("No workspace symbols found matching query");
            return Ok(None);
        }

        tracing::debug!("Returning {} workspace symbols", all_symbols.len());
        Ok(Some(OneOf::Right(all_symbols)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        tracing::debug!("Semantic tokens requested: {:?}", uri);

        // Find workspace for this document
        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get AnalysisHost and create snapshot with timeout to avoid blocking during init
        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let file_path = graphql_ide::FilePath::new(uri.to_string());

        // Get semantic tokens from Analysis
        let tokens = analysis.semantic_tokens(&file_path);

        if tokens.is_empty() {
            tracing::debug!("No semantic tokens found in document: {:?}", uri);
            return Ok(None);
        }

        // Convert to LSP delta-encoded format
        let mut encoded_tokens = Vec::with_capacity(tokens.len() * 5);
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;

        for token in tokens {
            let delta_line = token.start.line - prev_line;
            let delta_start = if delta_line == 0 {
                token.start.character - prev_start
            } else {
                token.start.character
            };

            encoded_tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type.index(),
                token_modifiers_bitset: token.modifiers.raw(),
            });

            prev_line = token.start.line;
            prev_start = token.start.character;
        }

        // Log any deprecated tokens for debugging
        let deprecated_count = encoded_tokens
            .iter()
            .filter(|t| t.token_modifiers_bitset != 0)
            .count();
        if deprecated_count > 0 {
            tracing::info!(
                "Found {} tokens with modifiers (deprecated or definition)",
                deprecated_count
            );
            for token in encoded_tokens
                .iter()
                .filter(|t| t.token_modifiers_bitset != 0)
            {
                tracing::info!(
                    "  Token with modifiers_bitset={}",
                    token.token_modifiers_bitset
                );
            }
        }

        tracing::debug!(
            "Returning {} semantic tokens for {:?}",
            encoded_tokens.len(),
            uri
        );
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: encoded_tokens,
        })))
    }

    #[allow(
        clippy::uninlined_format_args,
        clippy::single_match_else,
        clippy::option_if_let_else,
        clippy::manual_string_new,
        clippy::manual_map
    )]
    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        tracing::info!("Execute command requested: {}", params.command);

        if params.command.as_str() == "graphql.checkStatus" {
            let mut status_lines = Vec::new();

            for workspace_entry in &self.workspace.workspace_roots {
                let workspace_uri = workspace_entry.key();
                let workspace_path = workspace_entry.value();

                status_lines.push(format!("Workspace: {}", workspace_path.display()));

                if let Some(config_path) = self.workspace.config_paths.get(workspace_uri) {
                    status_lines.push(format!(
                        "  Config: {}",
                        config_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                    ));
                }

                // Project information removed (old system)
            }

            let status_report = status_lines.join("\n");

            let full_report = format!("\n=== GraphQL LSP Status ===\n{}\n", status_report);
            tracing::info!("{}", full_report);

            self.client
                .log_message(MessageType::INFO, full_report)
                .await;

            let summary = if self.workspace.workspace_roots.is_empty() {
                "No workspaces loaded".to_string()
            } else {
                let workspace_count = self.workspace.workspace_roots.len();
                format!(
                    "{} workspace(s) - Check output for details",
                    workspace_count
                )
            };

            self.client.show_message(MessageType::INFO, summary).await;

            Ok(Some(serde_json::json!({ "success": true })))
        } else {
            tracing::warn!("Unknown command: {}", params.command);
            Ok(None)
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::mutable_key_type,
        clippy::too_many_lines
    )]
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
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

        // Get lint diagnostics with fixes for this file (per-file rules)
        let mut lint_diagnostics = analysis.lint_diagnostics_with_fixes(&file_path);

        // Also get project-level diagnostics for this file (e.g., unused_fragments)
        let project_diagnostics = analysis.project_lint_diagnostics_with_fixes();
        if let Some(project_diags_for_file) = project_diagnostics.get(&file_path) {
            lint_diagnostics.extend(project_diags_for_file.iter().cloned());
        }

        if lint_diagnostics.is_empty() {
            return Ok(None);
        }

        // Convert LSP range to line/column for comparison
        let start_line = range.start.line as usize;
        let end_line = range.end.line as usize;

        // Filter diagnostics that overlap with the requested range
        // and have fixes available
        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        // Get file content for line index
        let Some(content) = analysis.file_content(&file_path) else {
            return Ok(None);
        };

        let file_line_index = graphql_syntax::LineIndex::new(&content);

        for diag in lint_diagnostics {
            // Skip diagnostics without fixes
            let Some(ref fix) = diag.fix else {
                continue;
            };

            // For embedded GraphQL (TypeScript/JavaScript), offsets are relative to the
            // GraphQL block, not the full file. Use block context when available.
            let (line_offset, diag_line_index): (
                usize,
                std::borrow::Cow<'_, graphql_syntax::LineIndex>,
            ) = if let (Some(block_line_offset), Some(ref block_source)) =
                (diag.block_line_offset, &diag.block_source)
            {
                (
                    block_line_offset,
                    std::borrow::Cow::Owned(graphql_syntax::LineIndex::new(block_source)),
                )
            } else {
                (0, std::borrow::Cow::Borrowed(&file_line_index))
            };

            // Convert diagnostic offset to line/column
            let (diag_start_line, _) = diag_line_index.line_col(diag.offset_range.start);
            let (diag_end_line, _) = diag_line_index.line_col(diag.offset_range.end);
            let diag_start_line = diag_start_line + line_offset;
            let diag_end_line = diag_end_line + line_offset;

            // Check if diagnostic overlaps with requested range
            if diag_end_line < start_line || diag_start_line > end_line {
                continue;
            }

            // Convert fix edits to LSP TextEdits
            let edits: Vec<TextEdit> = fix
                .edits
                .iter()
                .map(|edit| {
                    let (start_line, start_col) = diag_line_index.line_col(edit.offset_range.start);
                    let (end_line, end_col) = diag_line_index.line_col(edit.offset_range.end);

                    TextEdit {
                        range: lsp_types::Range {
                            start: lsp_types::Position {
                                line: (start_line + line_offset) as u32,
                                character: start_col as u32,
                            },
                            end: lsp_types::Position {
                                line: (end_line + line_offset) as u32,
                                character: end_col as u32,
                            },
                        },
                        new_text: edit.new_text.clone(),
                    }
                })
                .collect();

            // Create the workspace edit
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);

            let workspace_edit = WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            };

            // Create the code action
            let action = CodeAction {
                title: fix.label.clone(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![convert_ide_diagnostic(graphql_ide::Diagnostic {
                    range: graphql_ide::Range {
                        start: graphql_ide::Position {
                            line: diag_start_line as u32,
                            character: 0,
                        },
                        end: graphql_ide::Position {
                            line: diag_end_line as u32,
                            character: 0,
                        },
                    },
                    severity: graphql_ide::DiagnosticSeverity::Warning,
                    message: diag.message.clone(),
                    code: Some(diag.rule.clone()),
                    source: "graphql-linter".to_string(),
                    fix: None,
                })]),
                edit: Some(workspace_edit),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            };

            actions.push(CodeActionOrCommand::CodeAction(action));
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    #[tracing::instrument(skip(self, params), fields(uri = ?params.text_document.uri))]
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        tracing::debug!("Code lens requested: {:?}", uri);

        let Some((workspace_uri, project_name)) = self.workspace.find_workspace_and_project(&uri)
        else {
            tracing::debug!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        let host = self
            .workspace
            .get_or_create_host(&workspace_uri, &project_name);
        let Some(analysis) = host.try_snapshot().await else {
            return Ok(None);
        };

        let file_path = graphql_ide::FilePath::new(uri.to_string());
        let mut lsp_code_lenses: Vec<CodeLens> = Vec::new();

        // Code lenses for deprecated fields (in schema files)
        let deprecated_lenses = analysis.deprecated_field_code_lenses(&file_path);
        lsp_code_lenses.extend(
            deprecated_lenses
                .iter()
                .map(|cl| convert_ide_code_lens_info(cl, &uri)),
        );

        // Code lenses for fragment definitions (showing reference counts)
        let fragment_lenses = analysis.code_lenses(&file_path);
        for lens in &fragment_lenses {
            // Get fragment name from the command arguments (it's the 3rd argument)
            let fragment_name = lens
                .command
                .as_ref()
                .and_then(|cmd| cmd.arguments.get(2))
                .map(String::as_str);

            // Get references for this fragment
            let references: Vec<lsp_types::Location> = if let Some(name) = fragment_name {
                analysis
                    .find_fragment_references(name, false)
                    .iter()
                    .map(convert_ide_location)
                    .collect()
            } else {
                Vec::new()
            };

            lsp_code_lenses.push(convert_ide_code_lens(lens, &uri, &references));
        }

        if lsp_code_lenses.is_empty() {
            tracing::debug!("No code lenses found for {:?}", uri);
            return Ok(None);
        }

        tracing::debug!(
            "Returning {} code lenses for {:?}",
            lsp_code_lenses.len(),
            uri
        );
        Ok(Some(lsp_code_lenses))
    }

    async fn code_lens_resolve(&self, code_lens: CodeLens) -> Result<CodeLens> {
        // Code lens is already resolved with command, just return it
        Ok(code_lens)
    }
}
