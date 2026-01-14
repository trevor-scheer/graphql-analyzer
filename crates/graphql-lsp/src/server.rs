use crate::conversions::{
    convert_ide_completion_item, convert_ide_diagnostic, convert_ide_document_symbol,
    convert_ide_hover, convert_ide_location, convert_ide_workspace_symbol, convert_lsp_position,
};
use dashmap::DashMap;
use graphql_config::find_config;
use graphql_ide::AnalysisHost;
use lsp_types::{
    ClientCapabilities, CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FileChangeType,
    FileSystemWatcher, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, Location,
    MessageActionItem, MessageType, OneOf, ReferenceParams, ServerCapabilities, ServerInfo,
    SymbolInformation, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    WorkDoneProgressOptions, WorkspaceSymbol, WorkspaceSymbolParams,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::{Client, LanguageServer, UriExt};

pub struct GraphQLLanguageServer {
    client: Client,
    /// Client capabilities received during initialization
    client_capabilities: Arc<RwLock<Option<ClientCapabilities>>>,
    /// Workspace folders from initialization (stored temporarily until we load configs)
    init_workspace_folders: Arc<DashMap<String, PathBuf>>,
    /// Workspace roots indexed by workspace folder URI string
    workspace_roots: Arc<DashMap<String, PathBuf>>,
    /// Config file paths indexed by workspace URI string
    config_paths: Arc<DashMap<String, PathBuf>>,
    /// Loaded GraphQL configs indexed by workspace URI string
    configs: Arc<DashMap<String, graphql_config::GraphQLConfig>>,
    /// `AnalysisHost` per (workspace URI, project name) tuple
    #[allow(clippy::type_complexity)]
    hosts: Arc<DashMap<(String, String), Arc<Mutex<AnalysisHost>>>>,
    /// Document versions indexed by document URI string
    /// Used to detect out-of-order updates and avoid race conditions
    document_versions: Arc<DashMap<String, i32>>,
    /// Reverse index: file URI → (`workspace_uri`, `project_name`)
    /// Provides O(1) lookup instead of O(n) iteration over all hosts
    file_to_project: Arc<DashMap<String, (String, String)>>,
}

impl GraphQLLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            client_capabilities: Arc::new(RwLock::new(None)),
            init_workspace_folders: Arc::new(DashMap::new()),
            workspace_roots: Arc::new(DashMap::new()),
            config_paths: Arc::new(DashMap::new()),
            configs: Arc::new(DashMap::new()),
            hosts: Arc::new(DashMap::new()),
            document_versions: Arc::new(DashMap::new()),
            file_to_project: Arc::new(DashMap::new()),
        }
    }

    /// Get or create an `AnalysisHost` for a workspace/project
    fn get_or_create_host(
        &self,
        workspace_uri: &str,
        project_name: &str,
    ) -> Arc<Mutex<AnalysisHost>> {
        self.hosts
            .entry((workspace_uri.to_string(), project_name.to_string()))
            .or_insert_with(|| Arc::new(Mutex::new(AnalysisHost::new())))
            .clone()
    }

    /// Determine `FileKind` for a document file based on its path.
    ///
    /// This is used for files loaded from the `documents` configuration.
    /// - `.ts`/`.tsx` files → TypeScript
    /// - `.js`/`.jsx` files → JavaScript
    /// - `.graphql`/`.gql` files → `ExecutableGraphQL`
    ///
    /// Note: Files from the `schema` configuration are always `FileKind::Schema`,
    /// regardless of their extension.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn determine_file_kind(path: &str, _content: &str) -> graphql_ide::FileKind {
        if path.ends_with(".ts") || path.ends_with(".tsx") {
            graphql_ide::FileKind::TypeScript
        } else if path.ends_with(".js") || path.ends_with(".jsx") {
            graphql_ide::FileKind::JavaScript
        } else {
            graphql_ide::FileKind::ExecutableGraphQL
        }
    }
    /// Expand brace patterns like `{ts,tsx}` into multiple patterns
    ///
    /// This is needed because the glob crate doesn't support brace expansion.
    /// For example, `**/*.{ts,tsx}` expands to `["**/*.ts", "**/*.tsx"]`.
    fn expand_braces(pattern: &str) -> Vec<String> {
        if let Some(start) = pattern.find('{') {
            if let Some(end) = pattern.find('}') {
                let before = &pattern[..start];
                let after = &pattern[end + 1..];
                let options = &pattern[start + 1..end];

                return options
                    .split(',')
                    .map(|opt| format!("{before}{opt}{after}"))
                    .collect();
            }
        }

        vec![pattern.to_string()]
    }

    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    /// Load GraphQL config from a workspace folder and load all project files
    async fn load_workspace_config(&self, workspace_uri: &str, workspace_path: &PathBuf) {
        tracing::info!(path = ?workspace_path, "Loading GraphQL config");

        self.workspace_roots
            .insert(workspace_uri.to_string(), workspace_path.clone());

        match find_config(workspace_path) {
            Ok(Some(config_path)) => {
                self.config_paths
                    .insert(workspace_uri.to_string(), config_path.clone());

                match graphql_config::load_config(&config_path) {
                    Ok(config) => {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                "GraphQL config found, loading files...",
                            )
                            .await;

                        self.configs
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

    /// Load all GraphQL files from the config into `AnalysisHost`
    #[allow(clippy::too_many_lines)]
    async fn load_all_project_files(
        &self,
        workspace_uri: &str,
        workspace_path: &Path,
        config: &graphql_config::GraphQLConfig,
    ) {
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

            let host = self.get_or_create_host(workspace_uri, project_name);

            {
                let mut host_guard = host.lock().await;
                host_guard.set_extract_config(extract_config.clone());
                host_guard.set_lint_config(lint_config);
            }

            {
                let mut host_guard = host.lock().await;
                if let Err(e) = host_guard.load_schemas_from_config(project_config, workspace_path)
                {
                    tracing::error!("Failed to load schemas: {}", e);
                }
            }

            if let Some(documents_config) = &project_config.documents {
                const MAX_FILES_WARNING_THRESHOLD: usize = 1000;

                let patterns: Vec<String> = documents_config
                    .patterns()
                    .into_iter()
                    .map(std::string::ToString::to_string)
                    .collect();

                let mut collected_files: Vec<(
                    graphql_ide::FilePath,
                    String,
                    graphql_ide::FileKind,
                )> = Vec::new();
                let mut files_scanned = 0;

                for pattern in patterns {
                    if pattern.trim().starts_with('!') {
                        continue;
                    }

                    let expanded_patterns = Self::expand_braces(&pattern);

                    for expanded_pattern in expanded_patterns {
                        let full_pattern = workspace_path.join(&expanded_pattern);

                        match glob::glob(&full_pattern.display().to_string()) {
                            Ok(paths) => {
                                for entry in paths {
                                    match entry {
                                        Ok(path) if path.is_file() => {
                                            if path
                                                .components()
                                                .any(|c| c.as_os_str() == "node_modules")
                                            {
                                                continue;
                                            }

                                            files_scanned += 1;
                                            if files_scanned > 0 && files_scanned % 100 == 0 {
                                                tracing::info!(
                                                    "Scanned {} files so far (pattern: {})",
                                                    files_scanned,
                                                    pattern
                                                );

                                                // Show warning at threshold
                                                if files_scanned == MAX_FILES_WARNING_THRESHOLD {
                                                    tracing::warn!(
                                                        "Loading large number of files ({}+), this may take a while...",
                                                        MAX_FILES_WARNING_THRESHOLD
                                                    );
                                                    self.client
                                                        .show_message(
                                                            MessageType::WARNING,
                                                            format!(
                                                                "GraphQL LSP: Loading {MAX_FILES_WARNING_THRESHOLD}+ files, this may take a while. \
                                                                Consider using more specific patterns if this is too slow."
                                                            ),
                                                        )
                                                        .await;
                                                }
                                            }

                                            // Read file content (no lock held)
                                            match std::fs::read_to_string(&path) {
                                                Ok(content) => {
                                                    let path_str = path.display().to_string();
                                                    let file_kind = Self::determine_file_kind(
                                                        &path_str, &content,
                                                    );

                                                    // Use Uri::from_file_path for proper URI construction
                                                    // This ensures consistency with how VSCode formats URIs
                                                    let uri_string = if let Some(uri) =
                                                        Uri::from_file_path(&path)
                                                    {
                                                        uri.to_string()
                                                    } else {
                                                        // Fallback to manual construction
                                                        let path_str =
                                                            path_str.trim_start_matches('/');
                                                        format!("file:///{path_str}")
                                                    };
                                                    let file_path =
                                                        graphql_ide::FilePath::new(uri_string);

                                                    collected_files
                                                        .push((file_path, content, file_kind));
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        "Failed to read file {}: {}",
                                                        path.display(),
                                                        e
                                                    );
                                                }
                                            }
                                        }
                                        Ok(_) => {}
                                        Err(e) => {
                                            tracing::warn!("Glob entry error: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Invalid glob pattern '{}': {}",
                                    expanded_pattern,
                                    e
                                );
                            }
                        }
                    }
                }

                let total_files_loaded = collected_files.len();
                tracing::info!(
                    "Collected {} document files for project '{}', adding to host in batch...",
                    total_files_loaded,
                    project_name
                );

                {
                    let mut host_guard = host.lock().await;
                    for (file_path, content, file_kind) in &collected_files {
                        host_guard.add_file(file_path, content, *file_kind, 0);
                    }
                }

                for (file_path, _, _) in &collected_files {
                    self.file_to_project.insert(
                        file_path.as_str().to_string(),
                        (workspace_uri.to_string(), project_name.to_string()),
                    );
                }

                tracing::info!(
                    "Finished loading documents for project '{}': {} files total",
                    project_name,
                    total_files_loaded
                );

                tracing::info!(
                    "Rebuilding ProjectFiles index for {} files...",
                    total_files_loaded
                );
                let rebuild_start = std::time::Instant::now();
                {
                    let mut host_guard = host.lock().await;
                    host_guard.rebuild_project_files();
                }
                tracing::info!(
                    "ProjectFiles rebuild took {:.2}s",
                    rebuild_start.elapsed().as_secs_f64()
                );
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

        let Some(workspace_path) = self.workspace_roots.get(workspace_uri).map(|r| r.clone())
        else {
            tracing::error!(
                "Cannot reload config: workspace root not found for {}",
                workspace_uri
            );
            return;
        };

        let keys_to_remove: Vec<_> = self
            .hosts
            .iter()
            .filter(|entry| entry.key().0 == workspace_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for key in &keys_to_remove {
            tracing::debug!("Removing host for project: {}", key.1);
            self.hosts.remove(key);
        }

        tracing::info!(
            "Cleared {} existing host(s) for workspace",
            keys_to_remove.len()
        );

        let file_keys_to_remove: Vec<_> = self
            .file_to_project
            .iter()
            .filter(|entry| entry.value().0 == workspace_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for key in &file_keys_to_remove {
            self.file_to_project.remove(key);
        }

        tracing::info!(
            "Cleared {} file-to-project mappings for workspace",
            file_keys_to_remove.len()
        );

        self.configs.remove(workspace_uri);
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

    /// Find the workspace and project for a given document URI
    ///
    /// Uses a reverse index for O(1) lookup of previously seen files.
    /// Falls back to config pattern matching for files opened after init
    /// that haven't been indexed yet.
    fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
        let uri_string = document_uri.to_string();

        if let Some(entry) = self.file_to_project.get(&uri_string) {
            return Some(entry.value().clone());
        }

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

    /// Validate a file and publish diagnostics
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(path = ?uri.to_file_path().unwrap()))]
    async fn validate_file(&self, uri: Uri) {
        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No workspace/project found for file");
            return;
        };

        let Some(host_mutex) = self
            .hosts
            .get(&(workspace_uri.clone(), project_name.clone()))
        else {
            tracing::warn!("No analysis host found for workspace/project");
            return;
        };

        let snapshot = host_mutex.lock().await.snapshot();
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

        tracing::info!(
            supports_hover,
            supports_completion,
            supports_definition,
            supports_references,
            supports_document_symbols,
            supports_workspace_symbols,
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
                    self.init_workspace_folders
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

        // Load GraphQL config from workspace folders we stored during initialize
        let folders: Vec<_> = self
            .init_workspace_folders
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        tracing::info!("Loading configs for {} workspace(s)", folders.len());
        for (uri, path) in folders {
            tracing::info!(
                "Loading config for workspace: {} at {}",
                uri,
                path.display()
            );
            self.load_workspace_config(&uri, &path).await;
        }

        tracing::info!(
            "After loading: {} workspace roots, {} configs",
            self.workspace_roots.len(),
            self.configs.len()
        );

        // Register file watchers for config files after loading
        let config_paths: Vec<PathBuf> = self
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

        // Create file system watchers for all config files
        // Note: We use relative glob patterns from workspace root, not absolute file:// URIs
        // VSCode file watchers work better with relative patterns
        let watchers: Vec<FileSystemWatcher> = config_paths
            .iter()
            .filter_map(|path| {
                // Get the filename for the glob pattern
                let filename = path.file_name()?.to_str()?;

                tracing::debug!(
                    "Watching config file: {} (pattern: **/{filename})",
                    path.display()
                );

                // Use a glob pattern that matches this config file anywhere in the workspace
                // This works better than absolute URIs for workspace file watchers
                Some(FileSystemWatcher {
                    glob_pattern: lsp_types::GlobPattern::String(format!("**/{filename}")),
                    kind: Some(lsp_types::WatchKind::all()),
                })
            })
            .collect();

        // Register the watchers with the client
        let registration = lsp_types::Registration {
            id: "graphql-config-watcher".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(lsp_types::DidChangeWatchedFilesRegistrationOptions {
                    watchers,
                })
                .unwrap(),
            ),
        };

        let result = self.client.register_capability(vec![registration]).await;

        match result {
            Ok(()) => {
                tracing::info!("Successfully registered config file watchers");
            }
            Err(e) => {
                tracing::error!("Failed to register config file watchers: {:?}", e);
            }
        }
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

        self.document_versions.insert(uri.to_string(), version);

        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            self.validate_file(uri).await;
            return;
        };

        self.file_to_project.insert(
            uri.to_string(),
            (workspace_uri.clone(), project_name.clone()),
        );

        let host = self.get_or_create_host(&workspace_uri, &project_name);

        let file_kind =
            graphql_syntax::determine_file_kind_from_content(uri.path().as_str(), &content);

        // For TS/JS files, store the original source and let the parsing layer handle extraction.
        // This preserves block boundaries and allows proper validation of separate documents.
        let final_content = content;
        let line_offset = 0;
        let final_kind = file_kind;

        // === PHASE 3: Update file and get snapshot in one lock (optimized path) ===
        let file_path = graphql_ide::FilePath::new(uri.to_string());
        let snapshot = {
            let mut host_guard = host.lock().await;
            let (_is_new, snapshot) = host_guard.update_file_and_snapshot(
                &file_path,
                &final_content,
                final_kind,
                line_offset,
            );
            snapshot
        };

        // === PHASE 4: Validate using pre-acquired snapshot (no lock needed) ===
        self.validate_file_with_snapshot(&uri, snapshot).await;
    }

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        let uri_string = uri.to_string();
        if let Some(current_version) = self.document_versions.get(&uri_string) {
            if version <= *current_version {
                tracing::warn!(
                    "Ignoring stale document update: version {} <= current {}",
                    version,
                    *current_version
                );
                return;
            }
        }
        self.document_versions.insert(uri_string, version);

        for change in params.content_changes {
            let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
                continue;
            };

            let host = self.get_or_create_host(&workspace_uri, &project_name);

            let file_kind =
                graphql_syntax::determine_file_kind_from_content(uri.path().as_str(), &change.text);

            // For TS/JS files, store the original source and let the parsing layer handle extraction.
            // This preserves block boundaries and allows proper validation of separate documents.
            let final_content = change.text.clone();
            let line_offset = 0;
            let final_kind = file_kind;

            // === PHASE 3: Update file and get snapshot in one lock (optimized path) ===
            let file_path = graphql_ide::FilePath::new(uri.to_string());
            let snapshot = {
                let mut host_guard = host.lock().await;
                let (_is_new, snapshot) = host_guard.update_file_and_snapshot(
                    &file_path,
                    &final_content,
                    final_kind,
                    line_offset,
                );
                snapshot
            };

            // === PHASE 4: Validate using pre-acquired snapshot (no lock needed) ===
            self.validate_file_with_snapshot(&uri, snapshot).await;
        }
    }

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;

        // Find the workspace and project for this file
        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            tracing::debug!(
                "No workspace/project found for saved file, skipping project-wide lints"
            );
            return;
        };

        // Get the analysis host for this workspace/project
        let Some(host_mutex) = self
            .hosts
            .get(&(workspace_uri.clone(), project_name.clone()))
        else {
            tracing::debug!("No analysis host found for workspace/project");
            return;
        };

        // Run project-wide lints on save (these are expensive, so we don't run them on every change)
        let snapshot = host_mutex.lock().await.snapshot();
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
        self.document_versions
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

            let workspace_uri = self
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

        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            return Ok(None);
        };

        let host = self.get_or_create_host(&workspace_uri, &project_name);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
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

        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            return Ok(None);
        };

        let host = self.get_or_create_host(&workspace_uri, &project_name);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
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

        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            return Ok(None);
        };

        let host = self.get_or_create_host(&workspace_uri, &project_name);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
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

        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            return Ok(None);
        };

        let host = self.get_or_create_host(&workspace_uri, &project_name);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
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

        let Some((workspace_uri, project_name)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        let host = self.get_or_create_host(&workspace_uri, &project_name);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
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

        for entry in self.hosts.iter() {
            let host = entry.value();
            let analysis = {
                let host_guard = host.lock().await;
                host_guard.snapshot()
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

            for workspace_entry in self.workspace_roots.iter() {
                let workspace_uri = workspace_entry.key();
                let workspace_path = workspace_entry.value();

                status_lines.push(format!("Workspace: {}", workspace_path.display()));

                if let Some(config_path) = self.config_paths.get(workspace_uri) {
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

            let summary = if self.workspace_roots.is_empty() {
                "No workspaces loaded".to_string()
            } else {
                let workspace_count = self.workspace_roots.len();
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
}
