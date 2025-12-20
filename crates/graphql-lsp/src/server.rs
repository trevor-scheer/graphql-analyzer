// Allow nursery clippy lints that are too pedantic for our use case
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::significant_drop_in_scrutinee)]

use dashmap::DashMap;
use graphql_config::{find_config, load_config};
use graphql_linter::{LintConfig, Linter, ProjectContext};
use graphql_project::DynamicGraphQLProject;
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FileChangeType,
    FileSystemWatcher, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, Location,
    MessageType, NumberOrString, OneOf, Position, ProgressParams, ProgressParamsValue, Range,
    ReferenceParams, ServerCapabilities, ServerInfo, SymbolInformation, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri, WorkDoneProgress, WorkDoneProgressBegin, WorkDoneProgressEnd,
    WorkDoneProgressOptions, WorkspaceSymbol, WorkspaceSymbolParams,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::{Client, LanguageServer, UriExt};

/// Debounce delay for validation in milliseconds
const VALIDATION_DEBOUNCE_MS: u64 = 200;

/// Type alias for validation task handle
type ValidationTask = Arc<Mutex<Option<JoinHandle<()>>>>;

/// Type alias for a locked GraphQL project
type LockedProject = Arc<tokio::sync::RwLock<DynamicGraphQLProject>>;

/// Type alias for a list of projects in a workspace
/// Each project has a name, the project instance, and its own linter
type WorkspaceProjects = Vec<(String, LockedProject, Linter)>;

/// Type alias for config reload task handle
type ReloadTask = Arc<Mutex<Option<JoinHandle<()>>>>;

/// Load lint configuration for a specific project with LSP-specific overrides
///
/// Priority (highest to lowest):
/// 1. `extensions.lsp.lint` - LSP-specific overrides from project config
/// 2. Project-level `lint` - Project-specific defaults
fn load_lsp_lint_config_for_project(project: &DynamicGraphQLProject) -> LintConfig {
    // Get base lint config from project
    let base_config: LintConfig = project
        .lint_config()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();

    // Get LSP-specific overrides from extensions.lsp.lint
    let lsp_overrides = project
        .extensions()
        .and_then(|ext| ext.get("lsp"))
        .and_then(|lsp_ext| {
            if let serde_json::Value::Object(map) = lsp_ext {
                map.get("lint")
            } else {
                None
            }
        })
        .and_then(|value| serde_json::from_value::<LintConfig>(value.clone()).ok());

    // Merge: LSP-specific overrides take precedence over base config
    if let Some(overrides) = lsp_overrides {
        base_config.merge(&overrides)
    } else {
        base_config
    }
}

pub struct GraphQLLanguageServer {
    client: Client,
    /// Workspace folders from initialization (stored temporarily until we load configs)
    init_workspace_folders: Arc<DashMap<String, PathBuf>>,
    /// Workspace roots indexed by workspace folder URI string
    workspace_roots: Arc<DashMap<String, PathBuf>>,
    /// Config file paths indexed by workspace URI string
    config_paths: Arc<DashMap<String, PathBuf>>,
    /// GraphQL projects by workspace URI -> Vec<(`project_name`, project, linter)>
    /// Each project has its own linter configured from project-level lint config
    projects: Arc<DashMap<String, WorkspaceProjects>>,
    /// Document content cache indexed by URI string
    document_cache: Arc<DashMap<String, String>>,
    /// Pending validation tasks (URI -> `JoinHandle`) for debouncing
    /// Each document can have at most one pending validation task
    validation_tasks: Arc<DashMap<String, ValidationTask>>,
    /// Pending config reload tasks (workspace URI -> `JoinHandle`) for debouncing
    reload_tasks: Arc<DashMap<String, ReloadTask>>,
}

impl GraphQLLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            init_workspace_folders: Arc::new(DashMap::new()),
            workspace_roots: Arc::new(DashMap::new()),
            config_paths: Arc::new(DashMap::new()),
            projects: Arc::new(DashMap::new()),
            document_cache: Arc::new(DashMap::new()),
            validation_tasks: Arc::new(DashMap::new()),
            reload_tasks: Arc::new(DashMap::new()),
        }
    }

    /// Load GraphQL config from a workspace folder
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    async fn load_workspace_config(&self, workspace_uri: &str, workspace_path: &PathBuf) {
        tracing::info!(path = ?workspace_path, "Loading GraphQL config");

        // Find graphql config
        match find_config(workspace_path) {
            Ok(Some(config_path)) => {
                tracing::info!(config_path = ?config_path, "Found GraphQL config");

                // Store the config path for watching
                self.config_paths
                    .insert(workspace_uri.to_string(), config_path.clone());

                // Load the config
                match load_config(&config_path) {
                    Ok(config) => {
                        // Create and initialize projects from config
                        match DynamicGraphQLProject::from_config_with_base(&config, workspace_path)
                            .await
                        {
                            Ok(projects) => {
                                tracing::info!(
                                    count = projects.len(),
                                    "Initialized GraphQL projects"
                                );

                                // Log project info
                                for (name, project) in &projects {
                                    let doc_index = project.document_index();
                                    match doc_index.read() {
                                        Ok(doc_index_guard) => {
                                            tracing::info!(
                                                project = %name,
                                                operations = doc_index_guard.operations.len(),
                                                fragments = doc_index_guard.fragments.len(),
                                                "Project ready"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                project = %name,
                                                "Failed to acquire document index lock: {}",
                                                e
                                            );
                                        }
                                    };
                                }

                                // Wrap projects and create per-project linters
                                let wrapped_projects: Vec<(
                                    String,
                                    Arc<tokio::sync::RwLock<DynamicGraphQLProject>>,
                                    Linter,
                                )> = projects
                                    .into_iter()
                                    .map(|(name, proj)| {
                                        // Load lint config from project
                                        let lint_config = load_lsp_lint_config_for_project(&proj);
                                        let linter = Linter::new(lint_config);
                                        tracing::info!(project = %name, "Loaded lint configuration for project");
                                        (name, Arc::new(tokio::sync::RwLock::new(proj)), linter)
                                    })
                                    .collect();

                                // Store workspace and projects
                                self.workspace_roots
                                    .insert(workspace_uri.to_string(), workspace_path.clone());
                                self.projects
                                    .insert(workspace_uri.to_string(), wrapped_projects);

                                self.client
                                    .log_message(
                                        MessageType::INFO,
                                        "GraphQL config loaded successfully",
                                    )
                                    .await;
                            }
                            Err(e) => {
                                tracing::error!("Failed to initialize projects: {e}");
                                self.client
                                    .log_message(
                                        MessageType::ERROR,
                                        format!("Failed to load GraphQL projects: {e}"),
                                    )
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to load config: {e}");
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to parse GraphQL config: {e}"),
                            )
                            .await;
                    }
                }
            }
            Ok(None) => {
                tracing::warn!("No GraphQL config found in workspace");
                self.client
                    .log_message(
                        MessageType::WARNING,
                        "No graphql.config found. Place a graphql.config.yaml in your workspace root.",
                    )
                    .await;
            }
            Err(e) => {
                tracing::error!("Error searching for config: {}", e);
            }
        }
    }

    /// Reload GraphQL config for a workspace
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    async fn reload_workspace_config(&self, workspace_uri: &str) {
        tracing::info!("Reloading GraphQL config");

        // Get the config path
        let Some(config_path) = self.config_paths.get(workspace_uri).map(|r| r.clone()) else {
            tracing::warn!("No config path found for workspace");
            return;
        };

        let Some(workspace_path) = self.workspace_roots.get(workspace_uri).map(|r| r.clone())
        else {
            tracing::warn!("No workspace path found");
            return;
        };

        // Show progress notification with toast
        let token = NumberOrString::String(format!("graphql-config-reload-{workspace_uri}"));
        let create_progress = self
            .client
            .send_request::<lsp_types::request::WorkDoneProgressCreate>(
                lsp_types::WorkDoneProgressCreateParams {
                    token: token.clone(),
                },
            )
            .await;

        if create_progress.is_err() {
            tracing::warn!("Failed to create progress token");
        }

        // Send begin progress
        self.client
            .send_notification::<lsp_types::notification::Progress>(ProgressParams {
                token: token.clone(),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                    WorkDoneProgressBegin {
                        title: "GraphQL".to_string(),
                        message: Some("Config changed, reloading...".to_string()),
                        cancellable: Some(false),
                        percentage: None,
                    },
                )),
            })
            .await;

        // Try to load the new config
        match load_config(&config_path) {
            Ok(config) => {
                // Create and initialize projects from config
                match DynamicGraphQLProject::from_config_with_base(&config, &workspace_path).await {
                    Ok(projects) => {
                        tracing::info!(count = projects.len(), "Re-initialized GraphQL projects");

                        // Wrap projects and create per-project linters
                        let wrapped_projects: Vec<(
                            String,
                            Arc<tokio::sync::RwLock<DynamicGraphQLProject>>,
                            Linter,
                        )> = projects
                            .into_iter()
                            .map(|(name, proj)| {
                                // Load lint config from project
                                let lint_config = load_lsp_lint_config_for_project(&proj);
                                let linter = Linter::new(lint_config);
                                tracing::info!(project = %name, "Reloaded lint configuration for project");
                                (name, Arc::new(tokio::sync::RwLock::new(proj)), linter)
                            })
                            .collect();

                        // Replace projects
                        self.projects
                            .insert(workspace_uri.to_string(), wrapped_projects);

                        // Send completion progress
                        self.client
                            .send_notification::<lsp_types::notification::Progress>(
                                ProgressParams {
                                    token: token.clone(),
                                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                                        WorkDoneProgressEnd {
                                            message: Some(
                                                "Config reloaded successfully".to_string(),
                                            ),
                                        },
                                    )),
                                },
                            )
                            .await;

                        // Show success message as a toast
                        self.client
                            .show_message(MessageType::INFO, "GraphQL config reloaded successfully")
                            .await;

                        // Revalidate all open documents with the new config
                        tracing::info!("Starting re-validation after config reload");
                        self.revalidate_all_documents().await;
                        tracing::info!("Completed re-validation after config reload");
                    }
                    Err(e) => {
                        tracing::error!("Failed to initialize projects after reload: {e}");

                        // Send error progress
                        self.client
                            .send_notification::<lsp_types::notification::Progress>(
                                ProgressParams {
                                    token: token.clone(),
                                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                                        WorkDoneProgressEnd {
                                            message: Some(format!("Failed to reload: {e}")),
                                        },
                                    )),
                                },
                            )
                            .await;

                        self.client
                            .show_message(
                                MessageType::ERROR,
                                format!("Failed to reload GraphQL config: {e}"),
                            )
                            .await;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to load config after change: {e}");

                // Send error progress
                self.client
                    .send_notification::<lsp_types::notification::Progress>(ProgressParams {
                        token: token.clone(),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                            WorkDoneProgressEnd {
                                message: Some(format!("Failed to reload: {e}")),
                            },
                        )),
                    })
                    .await;

                self.client
                    .show_message(
                        MessageType::ERROR,
                        format!("Failed to parse GraphQL config: {e}"),
                    )
                    .await;
            }
        }
    }

    /// Schedule a debounced config reload for a workspace
    async fn schedule_config_reload(&self, workspace_uri: String) {
        // Get or create the task slot for this workspace
        let task_slot = self
            .reload_tasks
            .entry(workspace_uri.clone())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone();

        // Cancel any existing pending reload for this workspace
        {
            let mut task_guard = task_slot.lock().await;
            if let Some(existing_task) = task_guard.take() {
                existing_task.abort();
                tracing::debug!(workspace = %workspace_uri, "Cancelled previous reload task");
            }
        }

        // Clone necessary data for the async task
        let server = Self {
            client: self.client.clone(),
            init_workspace_folders: self.init_workspace_folders.clone(),
            workspace_roots: self.workspace_roots.clone(),
            config_paths: self.config_paths.clone(),
            projects: self.projects.clone(),
            document_cache: self.document_cache.clone(),
            validation_tasks: self.validation_tasks.clone(),
            reload_tasks: self.reload_tasks.clone(),
        };

        let workspace_uri_for_task = workspace_uri.clone();

        // Spawn a new debounced reload task (500ms delay to batch rapid changes)
        let task = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            tracing::debug!(workspace = %workspace_uri_for_task, "Debounce period elapsed, starting config reload");

            server
                .reload_workspace_config(&workspace_uri_for_task)
                .await;

            // Clear the task slot once reload completes
            if let Some(slot) = server.reload_tasks.get(&workspace_uri_for_task) {
                let mut task_guard = slot.lock().await;
                *task_guard = None;
            }
        });

        // Store the new task
        {
            let mut task_guard = task_slot.lock().await;
            *task_guard = Some(task);
        }

        tracing::debug!(workspace = %workspace_uri, "Scheduled debounced config reload");
    }

    /// Find the workspace and project for a given document URI
    fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, usize)> {
        let doc_path = document_uri.to_file_path()?;

        // Try to find which workspace this document belongs to
        for workspace_entry in self.workspace_roots.iter() {
            let workspace_uri = workspace_entry.key();
            let workspace_path = workspace_entry.value();

            if doc_path.as_ref().starts_with(workspace_path.as_path()) {
                // Found the workspace, return the workspace URI and project index (0 for now)
                // TODO: Match document to correct project based on includes/excludes
                return Some((workspace_uri.clone(), 0));
            }
        }

        None
    }

    /// Re-validate all open documents in all workspaces
    /// This is called after schema changes to update validation errors
    async fn revalidate_all_documents(&self) {
        let start = std::time::Instant::now();
        tracing::info!("Starting re-validation of all open documents after schema change");

        // Collect all URIs and their content from the document cache
        let documents: Vec<(String, String)> = self
            .document_cache
            .iter()
            .map(|entry| {
                let uri_str = entry.key().clone();
                let content = entry.value().clone();
                (uri_str, content)
            })
            .collect();

        tracing::info!("Re-validating {} open documents", documents.len());

        // Validate each document (skip schema files to avoid recursion)
        for (uri_str, content) in documents {
            // Parse the URI string - these are already valid URIs from the LSP
            if let Ok(uri) = serde_json::from_str::<Uri>(&format!("\"{uri_str}\"")) {
                // Skip schema files - they don't need revalidation after schema changes
                if let Some(path) = uri.to_file_path() {
                    // Check if this is a schema file by checking against all projects
                    let mut is_schema = false;
                    for workspace_projects in self.projects.iter() {
                        for (_, project, _) in workspace_projects.value() {
                            if project.read().await.is_schema_file(&path) {
                                is_schema = true;
                                break;
                            }
                        }
                        if is_schema {
                            break;
                        }
                    }

                    if is_schema {
                        tracing::debug!("Skipping schema file: {:?}", uri);
                        continue;
                    }
                }

                tracing::debug!("Re-validating document: {:?}", uri);
                // Don't trigger another revalidate_all_documents during batch revalidation
                self.validate_document_impl(uri, &content, false).await;
            } else {
                tracing::warn!("Failed to parse URI: {}", uri_str);
            }
        }

        tracing::info!(
            "Completed re-validation of all documents in {:?}",
            start.elapsed()
        );
    }

    /// Re-validate all fragment definition files in the project
    /// This is called after document changes to update unused fragment warnings
    async fn revalidate_fragment_files(&self, changed_uri: &Uri) {
        let start = std::time::Instant::now();
        tracing::info!(
            "Starting re-validation of fragment files for: {:?}",
            changed_uri
        );

        // Find the workspace and project for the changed document
        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(changed_uri)
        else {
            tracing::debug!("No workspace found for URI: {:?}", changed_uri);
            return;
        };

        // Get all fragment files from the document index
        // We need to collect the file paths and then drop the borrow before validating
        let index_start = std::time::Instant::now();
        let fragment_files: std::collections::HashSet<String> = {
            let Some(projects) = self.projects.get(&workspace_uri) else {
                tracing::debug!("No projects loaded for workspace: {}", workspace_uri);
                return;
            };

            let Some((_, project, _)) = projects.get(project_idx) else {
                tracing::debug!(
                    "Project index {} not found in workspace {}",
                    project_idx,
                    workspace_uri
                );
                return;
            };

            let project_guard = project.read().await;
            let document_index = project_guard.document_index();
            let document_index_guard = match document_index.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("Failed to acquire document index lock: {}", e);
                    return;
                }
            };
            tracing::debug!("Got document index in {:?}", index_start.elapsed());

            document_index_guard
                .fragments
                .values()
                .flatten() // Flatten Vec<FragmentInfo> to iterate over each FragmentInfo
                .map(|frag_info| frag_info.file_path.clone())
                .collect()
        }; // Drop the borrow here before we start validating

        tracing::info!(
            "Re-validating {} fragment files after document change",
            fragment_files.len()
        );

        // Re-validate each fragment file
        for file_path in fragment_files {
            let file_start = std::time::Instant::now();
            tracing::debug!("Re-validating fragment file: {}", file_path);

            // Convert file path to URI
            let Some(fragment_uri) = Uri::from_file_path(&file_path) else {
                tracing::warn!("Failed to convert file path to URI: {}", file_path);
                continue;
            };

            // Get content from cache or read from disk
            let content =
                if let Some(cached_content) = self.document_cache.get(&fragment_uri.to_string()) {
                    tracing::debug!("Using cached content for: {}", file_path);
                    cached_content.clone()
                } else {
                    // Fragment file not open in editor, read from disk
                    tracing::debug!("Reading fragment file from disk: {}", file_path);
                    match std::fs::read_to_string(&file_path) {
                        Ok(content) => content,
                        Err(e) => {
                            tracing::warn!("Failed to read fragment file {}: {}", file_path, e);
                            continue;
                        }
                    }
                };

            // Validate the fragment file
            self.validate_document(fragment_uri, &content).await;
            tracing::debug!(
                "Validated fragment file {} in {:?}",
                file_path,
                file_start.elapsed()
            );
        }

        tracing::info!(
            "Completed re-validation of fragment files in {:?}",
            start.elapsed()
        );
    }

    /// Re-validate schema files when field usage changes in a document
    ///
    /// When operations or fragments change, the set of used fields changes,
    /// which affects `unused_fields` diagnostics in schema files. This method
    /// finds all schema files and re-publishes their diagnostics.
    async fn revalidate_schema_files(&self, changed_uri: &Uri) {
        let start = std::time::Instant::now();
        tracing::debug!(
            "Starting re-validation of schema files for: {:?}",
            changed_uri
        );

        // Find the workspace and project for the changed document
        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(changed_uri)
        else {
            tracing::debug!("No workspace found for URI: {:?}", changed_uri);
            return;
        };

        // Get all schema files from the project config
        let schema_files: Vec<String> = {
            let project = {
                let Some(projects) = self.projects.get(&workspace_uri) else {
                    tracing::debug!("No projects loaded for workspace: {}", workspace_uri);
                    return;
                };

                let Some((_, project, _)) = projects.get(project_idx) else {
                    tracing::debug!(
                        "Project index {} not found in workspace {}",
                        project_idx,
                        workspace_uri
                    );
                    return;
                };

                project.clone()
            };

            let schema_files = project.read().await.get_schema_file_paths();
            schema_files
        };

        tracing::debug!(
            "Re-validating {} schema files after field usage change",
            schema_files.len()
        );

        // Re-publish diagnostics for each schema file
        for file_path in schema_files {
            // Convert file path to URI
            let Some(schema_uri) = Uri::from_file_path(&file_path) else {
                tracing::warn!("Failed to convert schema file path to URI: {}", file_path);
                continue;
            };

            // Get content from cache or read from disk
            let content =
                if let Some(cached_content) = self.document_cache.get(&schema_uri.to_string()) {
                    cached_content.clone()
                } else {
                    // Schema file not open in editor, skip it
                    // We only update diagnostics for open files
                    continue;
                };

            // Re-validate the schema file (this will publish updated diagnostics)
            self.validate_document(schema_uri, &content).await;
        }

        tracing::debug!(
            "Completed re-validation of schema files in {:?}",
            start.elapsed()
        );
    }

    /// Validate a document and publish diagnostics
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self, content), fields(uri = ?uri))]
    async fn validate_document(&self, uri: Uri, content: &str) {
        self.validate_document_impl(uri, content, true).await;
    }

    /// Internal implementation of `validate_document` with control over revalidation
    #[allow(clippy::too_many_lines)]
    async fn validate_document_impl(&self, uri: Uri, content: &str, should_revalidate_all: bool) {
        let start = std::time::Instant::now();
        tracing::debug!("Validating document");

        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document");
            return;
        };

        let file_path = uri.to_file_path();

        // Check if this is a schema file and update document index
        // We do this in a narrow scope to minimize lock duration
        {
            // Get the project from the workspace (need mutable to update document index)
            let project = {
                let Some(mut projects) = self.projects.get_mut(&workspace_uri) else {
                    tracing::warn!("No projects loaded for workspace: {workspace_uri}");
                    return;
                };

                let Some((_, project, _)) = projects.get_mut(project_idx) else {
                    tracing::warn!(
                        "Project index {project_idx} not found in workspace {workspace_uri}"
                    );
                    return;
                };

                project.clone()
            };

            // Check if this is a schema file - schema files need special handling
            let is_schema_file = if let Some(path) = file_path.as_ref() {
                project.read().await.is_schema_file(path.as_ref())
            } else {
                false
            };

            if is_schema_file {
                tracing::info!("Schema file changed, reloading schema");

                // Update the schema index with the new content
                let file_path_buf = file_path.as_ref().unwrap().clone();
                if let Err(e) = project
                    .write()
                    .await
                    .update_schema_index(&file_path_buf, content)
                    .await
                {
                    tracing::error!("Failed to update schema: {}", e);
                    self.client
                        .log_message(MessageType::ERROR, format!("Failed to update schema: {e}"))
                        .await;
                    return;
                }

                tracing::info!("Schema reloaded successfully");

                // Publish project-wide lint diagnostics for the schema file
                // This includes unused_fields warnings
                let schema_diagnostics = async {
                    let Some(projects) = self.projects.get(&workspace_uri) else {
                        tracing::warn!("No projects loaded for workspace: {workspace_uri}");
                        return Vec::new();
                    };

                    let Some((_, project, linter)) = projects.get(project_idx) else {
                        tracing::warn!(
                            "Project index {project_idx} not found in workspace {workspace_uri}"
                        );
                        return Vec::new();
                    };

                    if let Some(path) = file_path.as_ref() {
                        let file_path_str = path.display().to_string();
                        self.get_project_wide_diagnostics(&file_path_str, project, linter)
                            .await
                    } else {
                        Vec::new()
                    }
                }
                .await;

                self.client
                    .publish_diagnostics(uri.clone(), schema_diagnostics, None)
                    .await;

                // After schema changes, we need to revalidate all open documents
                // because field types, deprecations, etc. may have changed
                // Only do this if we're not already in a batch revalidation
                if should_revalidate_all {
                    Box::pin(self.revalidate_all_documents()).await;
                }
                return;
            }

            // Update the document index for this specific file with in-memory content
            // This is more efficient than reloading all documents from disk and
            // ensures we use the latest editor content even before it's saved
            if let Some(path) = &file_path {
                let update_result = project.write().await.update_document_index(path, content);
                if let Err(e) = update_result {
                    tracing::warn!(
                        "Failed to update document index for {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        } // Drop the mutable lock here before validation

        // Check if this is a TypeScript/JavaScript file
        let is_ts_js = file_path
            .as_ref()
            .and_then(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| matches!(ext, "ts" | "tsx" | "js" | "jsx"))
            })
            .unwrap_or(false);

        // Get document-specific diagnostics (type errors, etc.)
        // Now we get a read-only reference for validation, which won't block other operations
        let mut diagnostics = {
            let Some(projects) = self.projects.get(&workspace_uri) else {
                tracing::warn!("No projects loaded for workspace: {workspace_uri}");
                return;
            };

            let Some((_, project, linter)) = projects.get(project_idx) else {
                tracing::warn!(
                    "Project index {project_idx} not found in workspace {workspace_uri}"
                );
                return;
            };

            if is_ts_js {
                self.validate_typescript_document(&uri, content, project, linter)
                    .await
            } else {
                self.validate_graphql_document(content, project, linter)
                    .await
            }
        }; // Drop the read lock here

        // Add project-wide duplicate name diagnostics for this file
        if let Some(path) = uri.to_file_path() {
            let file_path_str = path.display().to_string();

            let project_wide_diags = {
                let Some(projects) = self.projects.get(&workspace_uri) else {
                    tracing::warn!("No projects loaded for workspace: {workspace_uri}");
                    return;
                };

                let Some((_, project, linter)) = projects.get(project_idx) else {
                    tracing::warn!(
                        "Project index {project_idx} not found in workspace {workspace_uri}"
                    );
                    return;
                };

                self.get_project_wide_diagnostics(&file_path_str, project, linter)
                    .await
            }; // Drop the read lock here

            diagnostics.extend(project_wide_diags);
        }

        // Filter out diagnostics with invalid ranges (defensive fix for stale diagnostics)
        // Count total lines in the content to validate ranges
        let line_count = content.lines().count();
        diagnostics.retain(|diag| {
            let start_line = diag.range.start.line as usize;
            let end_line = diag.range.end.line as usize;

            // Keep diagnostic only if both start and end are within document bounds
            if start_line >= line_count || end_line >= line_count {
                tracing::warn!(
                    "Filtered out diagnostic with invalid range: {:?} (document has {} lines)",
                    diag.range,
                    line_count
                );
                false
            } else {
                true
            }
        });

        self.client
            .publish_diagnostics(uri.clone(), diagnostics.clone(), None)
            .await;

        tracing::debug!(
            elapsed_ms = start.elapsed().as_millis(),
            diagnostic_count = diagnostics.len(),
            "Validated document"
        );

        // Refresh diagnostics for any other files affected by duplicate name changes
        self.refresh_affected_files_diagnostics(&workspace_uri, project_idx, &uri)
            .await;
    }

    /// Get project-wide lint diagnostics for a specific file
    async fn get_project_wide_diagnostics(
        &self,
        file_path: &str,
        project: &Arc<tokio::sync::RwLock<DynamicGraphQLProject>>,
        linter: &Linter,
    ) -> Vec<Diagnostic> {
        // Run project-wide lint rules using graphql-linter
        let project_guard = project.read().await;
        let document_index = project_guard.document_index();
        let schema_index = project_guard.schema_index();

        let document_index_guard = match document_index.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire document index lock: {}", e);
                return Vec::new();
            }
        };
        let schema_index_guard = match schema_index.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire schema index lock: {}", e);
                return Vec::new();
            }
        };

        let ctx = ProjectContext {
            documents: &document_index_guard,
            schema: &schema_index_guard,
        };

        let project_diagnostics_by_file = linter.lint_project(&ctx);

        // Get diagnostics for this specific file
        project_diagnostics_by_file
            .get(file_path)
            .map(|diags| {
                diags
                    .iter()
                    .map(|diag| self.convert_project_diagnostic(diag.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Refresh diagnostics for all files affected by project-wide lint changes
    ///
    /// When a file is edited and introduces or removes issues detected by project-wide
    /// lints (like duplicate names or unused fields), other files that are affected
    /// need to have their diagnostics refreshed.
    #[allow(clippy::too_many_lines)]
    async fn refresh_affected_files_diagnostics(
        &self,
        workspace_uri: &str,
        project_idx: usize,
        changed_file_uri: &Uri,
    ) {
        use std::collections::HashSet;

        // Get the project and linter
        let Some(projects) = self.projects.get(workspace_uri) else {
            return;
        };

        let Some((_, project, linter)) = projects.get(project_idx) else {
            return;
        };

        // Run project-wide lints to get all affected files
        let affected_files: HashSet<String> = {
            let project_guard = project.read().await;
            let document_index = project_guard.document_index();
            let schema_index = project_guard.schema_index();
            let document_index_guard = match document_index.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("Failed to acquire document index lock: {}", e);
                    return;
                }
            };
            let schema_index_guard = match schema_index.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("Failed to acquire schema index lock: {}", e);
                    return;
                }
            };
            let ctx = ProjectContext {
                documents: &document_index_guard,
                schema: &schema_index_guard,
            };
            let project_diagnostics_by_file = linter.lint_project(&ctx);

            // Extract unique file paths that have project-wide diagnostics
            project_diagnostics_by_file.keys().cloned().collect()
        }; // Drop guards here

        let changed_file_path = changed_file_uri.to_file_path();

        // For each affected file (excluding the one we just validated), refresh diagnostics
        for file_path in affected_files {
            // Skip the file we just validated
            if let Some(ref changed_path) = changed_file_path {
                if file_path == changed_path.display().to_string() {
                    continue;
                }
            }

            // Try to convert the file path to a URI
            let Some(file_uri) = Uri::from_file_path(&file_path) else {
                tracing::warn!("Failed to convert file path to URI: {}", file_path);
                continue;
            };

            // Get the document content from cache, or read from disk
            let content = if let Some(cached) = self.document_cache.get(file_uri.as_str()) {
                cached.clone()
            } else {
                // File not in cache, try to read from disk
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => content,
                    Err(e) => {
                        tracing::warn!("Failed to read file {}: {}", file_path, e);
                        continue;
                    }
                }
            };

            // Check if this is a schema file
            let is_schema_file = project
                .read()
                .await
                .is_schema_file(std::path::Path::new(&file_path));

            // For schema files, only publish project-wide diagnostics (unused_fields)
            // Schema files should not be validated as executable documents
            let mut diagnostics = if is_schema_file {
                // Schema files only get project-wide diagnostics
                self.get_project_wide_diagnostics(&file_path, project, linter)
                    .await
            } else {
                // Check if this is a TypeScript/JavaScript file
                let is_ts_js = std::path::Path::new(&file_path)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| matches!(ext, "ts" | "tsx" | "js" | "jsx"));

                // Get document-specific diagnostics (type errors, etc.)
                let mut diagnostics = if is_ts_js {
                    self.validate_typescript_document(&file_uri, &content, project, linter)
                        .await
                } else {
                    self.validate_graphql_document(&content, project, linter)
                        .await
                };

                // Add project-wide diagnostics for this file
                let project_wide_diags = self
                    .get_project_wide_diagnostics(&file_path, project, linter)
                    .await;
                diagnostics.extend(project_wide_diags);
                diagnostics
            };

            // Filter out diagnostics with invalid ranges
            let line_count = content.lines().count();
            diagnostics.retain(|diag| {
                let start_line = diag.range.start.line as usize;
                let end_line = diag.range.end.line as usize;

                if start_line >= line_count || end_line >= line_count {
                    tracing::warn!(
                        "Filtered out diagnostic with invalid range: {:?} (document has {} lines)",
                        diag.range,
                        line_count
                    );
                    false
                } else {
                    true
                }
            });

            // Publish diagnostics for the affected file
            tracing::debug!(
                "Refreshing diagnostics for affected file: {} ({} diagnostics)",
                file_path,
                diagnostics.len()
            );
            self.client
                .publish_diagnostics(file_uri, diagnostics, None)
                .await;
        }
    }

    /// Validate a pure GraphQL document
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    async fn validate_graphql_document(
        &self,
        content: &str,
        project: &Arc<tokio::sync::RwLock<DynamicGraphQLProject>>,
        linter: &Linter,
    ) -> Vec<Diagnostic> {
        // Use the centralized validation logic from graphql-project
        let project_guard = project.read().await;
        let mut project_diagnostics =
            project_guard.validate_document_source(content, "document.graphql");

        // Run document-level lints using graphql-linter
        let schema_index = project_guard.schema_index();
        let schema_index_guard = match schema_index.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire schema index lock: {}", e);
                return project_diagnostics
                    .into_iter()
                    .map(|d| self.convert_project_diagnostic(d))
                    .collect();
            }
        };
        let document_index = project_guard.document_index();
        let document_index_guard = match document_index.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire document index lock: {}", e);
                return project_diagnostics
                    .into_iter()
                    .map(|d| self.convert_project_diagnostic(d))
                    .collect();
            }
        };

        // Run standalone document rules (don't need schema, but need fragments)
        let standalone_diagnostics = linter.lint_standalone_document(
            content,
            "document.graphql",
            Some(&document_index_guard),
            None,
        );
        project_diagnostics.extend(standalone_diagnostics);

        // Run document+schema rules
        let lint_diagnostics = linter.lint_document(
            content,
            "document.graphql",
            &schema_index_guard,
            Some(&document_index_guard),
            None,
        );
        project_diagnostics.extend(lint_diagnostics);

        // Convert graphql-project diagnostics to LSP diagnostics
        project_diagnostics
            .into_iter()
            .map(|d| self.convert_project_diagnostic(d))
            .collect()
    }

    /// Validate GraphQL embedded in TypeScript/JavaScript
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)]
    async fn validate_typescript_document(
        &self,
        uri: &Uri,
        content: &str,
        project: &Arc<tokio::sync::RwLock<DynamicGraphQLProject>>,
        linter: &Linter,
    ) -> Vec<Diagnostic> {
        use std::io::Write;

        // Get the file extension from the original URI to preserve it in the temp file
        let extension = uri
            .to_file_path()
            .and_then(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "tsx".to_string());

        let temp_file = match tempfile::Builder::new()
            .suffix(&format!(".{extension}"))
            .tempfile()
        {
            Ok(mut file) => {
                if file.write_all(content.as_bytes()).is_err() {
                    return vec![];
                }
                file
            }
            Err(_) => return vec![],
        };

        // Extract GraphQL from TypeScript/JavaScript
        let project_guard = project.read().await;
        let extract_config = project_guard.get_extract_config();
        let extracted = match graphql_extract::extract_from_file(temp_file.path(), &extract_config)
        {
            Ok(extracted) => extracted,
            Err(e) => {
                tracing::error!("Failed to extract GraphQL from {:?}: {}", uri, e);
                return vec![];
            }
        };

        if extracted.is_empty() {
            return vec![];
        }

        tracing::info!(
            "Extracted {} GraphQL document(s) from {:?}",
            extracted.len(),
            uri
        );

        // Use the centralized validation logic from graphql-project (Apollo compiler)
        let file_path = uri.to_string();
        let mut all_diagnostics =
            project_guard.validate_extracted_documents(&extracted, &file_path);

        // Run document-level lints using graphql-linter
        let schema_index = project_guard.schema_index();
        let schema_index_guard = match schema_index.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire schema index lock: {}", e);
                return all_diagnostics
                    .into_iter()
                    .map(|d| self.convert_project_diagnostic(d))
                    .collect();
            }
        };
        let document_index = project_guard.document_index();
        let document_index_guard = match document_index.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire document index lock: {}", e);
                return all_diagnostics
                    .into_iter()
                    .map(|d| self.convert_project_diagnostic(d))
                    .collect();
            }
        };

        for block in &extracted {
            // Run standalone document rules (don't need schema, but need fragments)
            let mut standalone_diagnostics = linter.lint_standalone_document(
                &block.source,
                &file_path,
                Some(&document_index_guard),
                None,
            );

            // Adjust positions for extracted blocks
            for diag in &mut standalone_diagnostics {
                diag.range.start.line += block.location.range.start.line;
                diag.range.end.line += block.location.range.start.line;

                // Adjust column only for first line
                if diag.range.start.line == block.location.range.start.line {
                    diag.range.start.character += block.location.range.start.column;
                }
                if diag.range.end.line == block.location.range.start.line {
                    diag.range.end.character += block.location.range.start.column;
                }
            }
            all_diagnostics.extend(standalone_diagnostics);

            // Run document+schema rules
            let lint_diagnostics = linter.lint_document(
                &block.source,
                &file_path,
                &schema_index_guard,
                Some(&document_index_guard),
                None,
            );

            // Adjust positions for extracted blocks
            for mut diag in lint_diagnostics {
                diag.range.start.line += block.location.range.start.line;
                diag.range.end.line += block.location.range.start.line;

                // Adjust column only for first line
                if diag.range.start.line == block.location.range.start.line {
                    diag.range.start.character += block.location.range.start.column;
                }
                if diag.range.end.line == block.location.range.start.line {
                    diag.range.end.character += block.location.range.start.column;
                }

                all_diagnostics.push(diag);
            }
        }

        // Convert graphql-project diagnostics to LSP diagnostics
        all_diagnostics
            .into_iter()
            .map(|d| self.convert_project_diagnostic(d))
            .collect()
    }

    /// Convert graphql-project diagnostic to LSP diagnostic
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::unused_self)]
    fn convert_project_diagnostic(&self, diag: graphql_project::Diagnostic) -> Diagnostic {
        use graphql_project::Severity;

        let severity = match diag.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Information => DiagnosticSeverity::INFORMATION,
            Severity::Hint => DiagnosticSeverity::HINT,
        };

        Diagnostic {
            range: Range {
                start: Position {
                    line: diag.range.start.line as u32,
                    character: diag.range.start.character as u32,
                },
                end: Position {
                    line: diag.range.end.line as u32,
                    character: diag.range.end.character as u32,
                },
            },
            severity: Some(severity),
            code: diag.code.map(lsp_types::NumberOrString::String),
            source: Some(diag.source),
            message: diag.message,
            ..Default::default()
        }
    }

    /// Schedule a debounced validation for a document
    ///
    /// Cancels any pending validation for the same document and schedules a new one
    /// after `VALIDATION_DEBOUNCE_MS` milliseconds. This prevents validation spam during
    /// rapid typing.
    async fn schedule_debounced_validation(&self, uri: Uri, content: String) {
        let uri_string = uri.to_string();

        // Get or create the task slot for this document
        let task_slot = self
            .validation_tasks
            .entry(uri_string.clone())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone();

        // Cancel any existing pending validation for this document
        {
            let mut task_guard = task_slot.lock().await;
            if let Some(existing_task) = task_guard.take() {
                existing_task.abort();
                tracing::debug!(uri = ?uri, "Cancelled previous validation task");
            }
        }

        // Clone necessary data for the async task
        let server = Self {
            client: self.client.clone(),
            init_workspace_folders: self.init_workspace_folders.clone(),
            workspace_roots: self.workspace_roots.clone(),
            config_paths: self.config_paths.clone(),
            projects: self.projects.clone(),
            document_cache: self.document_cache.clone(),
            validation_tasks: self.validation_tasks.clone(),
            reload_tasks: self.reload_tasks.clone(),
        };

        // Clone uri for the closure
        let uri_for_task = uri.clone();

        // Spawn a new debounced validation task
        let task = tokio::spawn(async move {
            // Wait for the debounce period
            tokio::time::sleep(tokio::time::Duration::from_millis(VALIDATION_DEBOUNCE_MS)).await;

            tracing::debug!(uri = ?uri_for_task, delay_ms = VALIDATION_DEBOUNCE_MS, "Debounce period elapsed, starting validation");

            let validate_start = std::time::Instant::now();
            server
                .validate_document(uri_for_task.clone(), &content)
                .await;
            tracing::debug!(uri = ?uri_for_task, elapsed_ms = validate_start.elapsed().as_millis(), "Main validation completed");

            // Re-validate all fragment definition files to update unused fragment warnings
            // This ensures that when fragment usage changes in one file, warnings in
            // fragment files are immediately updated
            let revalidate_start = std::time::Instant::now();
            server.revalidate_fragment_files(&uri_for_task).await;
            tracing::debug!(
                uri = ?uri_for_task,
                elapsed_ms = revalidate_start.elapsed().as_millis(),
                "Fragment revalidation completed"
            );

            // Clear the task slot once validation completes
            if let Some(slot) = server.validation_tasks.get(&uri_for_task.to_string()) {
                let mut task_guard = slot.lock().await;
                *task_guard = None;
            }
        });

        // Store the new task
        {
            let mut task_guard = task_slot.lock().await;
            *task_guard = Some(task);
        }

        tracing::debug!(uri = ?uri, delay_ms = VALIDATION_DEBOUNCE_MS, "Scheduled debounced validation");
    }
}

impl LanguageServer for GraphQLLanguageServer {
    #[tracing::instrument(skip(self, params))]
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing GraphQL Language Server");

        // Store workspace folders for later config loading
        if let Some(ref folders) = params.workspace_folders {
            tracing::info!(count = folders.len(), "Workspace folders");
            for folder in folders {
                if let Some(path) = folder.uri.to_file_path() {
                    self.init_workspace_folders
                        .insert(folder.uri.to_string(), path.into_owned());
                }
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["{".to_string(), "@".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
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

        for (uri, path) in folders {
            self.load_workspace_config(&uri, &path).await;
        }

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

    #[tracing::instrument(skip(self, params), fields(uri = ?params.text_document.uri))]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        tracing::info!("Document opened");

        // Cache the document content
        self.document_cache.insert(uri.to_string(), content.clone());

        self.validate_document(uri, &content).await;
    }

    #[tracing::instrument(skip(self, params), fields(uri = ?params.text_document.uri))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let start = std::time::Instant::now();
        tracing::info!("Document changed");

        // Get the latest content from changes (full sync mode)
        for change in params.content_changes {
            // Update the document cache
            self.document_cache
                .insert(uri.to_string(), change.text.clone());

            // Schedule debounced validation instead of immediate validation
            self.schedule_debounced_validation(uri.clone(), change.text.clone())
                .await;
        }

        tracing::debug!(
            elapsed_ms = start.elapsed().as_millis(),
            "Completed did_change (scheduled debounced validation)"
        );
    }

    #[tracing::instrument(skip(self, params), fields(uri = ?params.text_document.uri))]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        tracing::info!("Document saved");

        // Re-validate schema files to update unused_fields warnings
        // We do this on save (not on every keystroke) to avoid performance issues
        // When field usage changes in operations/fragments, unused_fields diagnostics
        // in schema files need to be updated
        //
        // Only revalidate schema files if the saved file is NOT a schema file
        // (schema file changes trigger full revalidation via validate_document_impl)
        let uri = params.text_document.uri;

        // Check if this is a schema file
        let is_schema_file = {
            let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(&uri) else {
                return;
            };

            let Some(projects) = self.projects.get(&workspace_uri) else {
                return;
            };

            let Some((_, project, _)) = projects.get(project_idx) else {
                return;
            };

            if let Some(path) = uri.to_file_path().as_ref() {
                project.read().await.is_schema_file(path.as_ref())
            } else {
                false
            }
        };

        // Only revalidate schema files if this is NOT a schema file
        if !is_schema_file {
            let schema_revalidate_start = std::time::Instant::now();
            self.revalidate_schema_files(&uri).await;
            tracing::debug!(
                "Schema revalidation took {:?}",
                schema_revalidate_start.elapsed()
            );
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        tracing::info!("Document closed: {:?}", params.text_document.uri);

        // Remove from document cache
        self.document_cache
            .remove(&params.text_document.uri.to_string());

        // Clear diagnostics
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        tracing::debug!("Watched files changed: {} file(s)", params.changes.len());

        // Process each file change
        for change in params.changes {
            let uri = change.uri;
            tracing::debug!("File changed: {:?} (type: {:?})", uri, change.typ);

            // Find which workspace this config belongs to
            let Some(config_path) = uri.to_file_path() else {
                tracing::warn!("Failed to convert URI to file path: {:?}", uri);
                continue;
            };

            // Find the workspace for this config file
            let workspace_uri = self
                .config_paths
                .iter()
                .find(|entry| entry.value() == &config_path)
                .map(|entry| entry.key().clone());

            if let Some(workspace_uri) = workspace_uri {
                match change.typ {
                    FileChangeType::CREATED | FileChangeType::CHANGED => {
                        tracing::info!("Scheduling config reload for workspace: {}", workspace_uri);
                        self.schedule_config_reload(workspace_uri).await;
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

        tracing::debug!("Completion requested: {:?} at {:?}", uri, lsp_position);

        let Some(content) = self.document_cache.get(&uri.to_string()) else {
            tracing::warn!("No cached content for document: {:?}", uri);
            return Ok(None);
        };

        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        let Some(projects) = self.projects.get(&workspace_uri) else {
            tracing::warn!("No projects loaded for workspace: {workspace_uri}");
            return Ok(None);
        };

        let Some((_, project, _)) = projects.get(project_idx) else {
            tracing::warn!("Project index {project_idx} not found in workspace {workspace_uri}");
            return Ok(None);
        };

        let position = graphql_project::Position {
            line: lsp_position.line as usize,
            character: lsp_position.character as usize,
        };

        let file_path = uri.to_string();
        let Some(items) = project
            .read()
            .await
            .complete(&content, position, &file_path)
        else {
            return Ok(None);
        };

        let lsp_items: Vec<lsp_types::CompletionItem> = items
            .into_iter()
            .map(|item| {
                let kind = match item.kind {
                    graphql_project::CompletionItemKind::Field => {
                        Some(lsp_types::CompletionItemKind::FIELD)
                    }
                    graphql_project::CompletionItemKind::Type => {
                        Some(lsp_types::CompletionItemKind::CLASS)
                    }
                    graphql_project::CompletionItemKind::Fragment => {
                        Some(lsp_types::CompletionItemKind::SNIPPET)
                    }
                    graphql_project::CompletionItemKind::Operation => {
                        Some(lsp_types::CompletionItemKind::FUNCTION)
                    }
                    graphql_project::CompletionItemKind::Directive => {
                        Some(lsp_types::CompletionItemKind::KEYWORD)
                    }
                    graphql_project::CompletionItemKind::EnumValue => {
                        Some(lsp_types::CompletionItemKind::ENUM_MEMBER)
                    }
                    graphql_project::CompletionItemKind::Argument => {
                        Some(lsp_types::CompletionItemKind::PROPERTY)
                    }
                    graphql_project::CompletionItemKind::Variable => {
                        Some(lsp_types::CompletionItemKind::VARIABLE)
                    }
                };

                let documentation = item.documentation.map(|doc| {
                    lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: doc,
                    })
                });

                lsp_types::CompletionItem {
                    label: item.label,
                    kind,
                    detail: item.detail,
                    documentation,
                    deprecated: Some(item.deprecated),
                    insert_text: item.insert_text,
                    ..Default::default()
                }
            })
            .collect();

        Ok(Some(CompletionResponse::Array(lsp_items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let lsp_position = params.text_document_position_params.position;

        tracing::debug!("Hover requested: {:?} at {:?}", uri, lsp_position);

        // Get the cached document content
        let Some(content) = self.document_cache.get(&uri.to_string()) else {
            tracing::warn!("No cached content for document: {:?}", uri);
            return Ok(None);
        };

        // Find the workspace and project for this document
        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get the project
        let Some(projects) = self.projects.get(&workspace_uri) else {
            tracing::warn!("No projects loaded for workspace: {workspace_uri}");
            return Ok(None);
        };

        let Some((_, project, _)) = projects.get(project_idx) else {
            tracing::warn!("Project index {project_idx} not found in workspace {workspace_uri}");
            return Ok(None);
        };

        // Convert LSP position to graphql-project Position
        let position = graphql_project::Position {
            line: lsp_position.line as usize,
            character: lsp_position.character as usize,
        };

        // Get hover info from the project (handles TypeScript extraction internally)
        // Convert URI to file path for cache lookup consistency
        let file_path = uri
            .to_file_path()
            .map_or_else(|| uri.to_string(), |path| path.display().to_string());

        let Some(hover_info) = project
            .read()
            .await
            .hover_info_at_position(&file_path, position, &content)
        else {
            return Ok(None);
        };

        // Convert to LSP Hover
        #[allow(clippy::cast_possible_truncation)]
        let hover = Hover {
            contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: hover_info.contents,
            }),
            range: hover_info.range.map(|r| Range {
                start: Position {
                    line: r.start.line as u32,
                    character: r.start.character as u32,
                },
                end: Position {
                    line: r.end.line as u32,
                    character: r.end.character as u32,
                },
            }),
        };

        Ok(Some(hover))
    }

    #[allow(clippy::too_many_lines)]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let start = std::time::Instant::now();
        let uri = params.text_document_position_params.text_document.uri;
        let lsp_position = params.text_document_position_params.position;

        tracing::info!(
            "Go to definition requested: {:?} at line={} char={}",
            uri,
            lsp_position.line,
            lsp_position.character
        );

        // Get the cached document content
        let Some(content) = self.document_cache.get(&uri.to_string()) else {
            tracing::warn!("No cached content for document: {:?}", uri);
            return Ok(None);
        };

        // Find the workspace and project for this document
        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get the project
        let Some(projects) = self.projects.get(&workspace_uri) else {
            tracing::warn!("No projects loaded for workspace: {workspace_uri}");
            return Ok(None);
        };

        let Some((_, project, _)) = projects.get(project_idx) else {
            tracing::warn!("Project index {project_idx} not found in workspace {workspace_uri}");
            return Ok(None);
        };

        // Check if this is a TypeScript/JavaScript file that needs GraphQL extraction
        let file_path = uri.to_file_path();
        let (is_ts_file, language) =
            file_path
                .as_ref()
                .map_or((false, graphql_extract::Language::GraphQL), |path| {
                    path.extension().and_then(|e| e.to_str()).map_or(
                        (false, graphql_extract::Language::GraphQL),
                        |ext| match ext {
                            "ts" | "tsx" => (true, graphql_extract::Language::TypeScript),
                            "js" | "jsx" => (true, graphql_extract::Language::JavaScript),
                            _ => (false, graphql_extract::Language::GraphQL),
                        },
                    )
                });

        if is_ts_file {
            // Try to use cached extracted blocks first (Phase 3 optimization)
            let project_guard = project.read().await;
            let cached_blocks = project_guard.get_extracted_blocks(&uri.to_string());

            // Find which extracted GraphQL block contains the cursor position
            let cursor_line = lsp_position.line as usize;

            if let Some(blocks) = cached_blocks {
                // Use cached blocks - no extraction or parsing needed!
                for block in blocks {
                    if cursor_line >= block.start_line && cursor_line <= block.end_line {
                        // Adjust position relative to the extracted GraphQL
                        #[allow(clippy::cast_possible_truncation)]
                        let relative_position = graphql_project::Position {
                            line: cursor_line - block.start_line,
                            character: if cursor_line == block.start_line {
                                lsp_position
                                    .character
                                    .saturating_sub(block.start_column as u32)
                                    as usize
                            } else {
                                lsp_position.character as usize
                            },
                        };

                        tracing::debug!(
                            "Using cached extracted block at position {:?}",
                            relative_position
                        );

                        // Get definition locations from the project using the cached GraphQL
                        let Some(locations) = project_guard.goto_definition(
                            &block.content,
                            relative_position,
                            &uri.to_string(),
                        ) else {
                            tracing::debug!(
                                "No definition found at position {:?}",
                                relative_position
                            );
                            continue;
                        };

                        tracing::debug!("Found {} definition location(s)", locations.len());

                        // Convert to LSP Locations (adjust positions back to original file coordinates)
                        #[allow(clippy::cast_possible_truncation)]
                        let lsp_locations: Vec<Location> = locations
                            .iter()
                            .filter_map(|loc| {
                                // Check if the file_path is already a URI
                                let file_uri = if loc.file_path.starts_with("file://") {
                                    // Already a URI, parse it directly
                                    loc.file_path.parse::<Uri>().ok()?
                                } else {
                                    // Resolve the file path relative to the workspace if it's not absolute
                                    let file_path = if std::path::Path::new(&loc.file_path)
                                        .is_absolute()
                                    {
                                        std::path::PathBuf::from(&loc.file_path)
                                    } else {
                                        // Resolve relative to workspace root
                                        let workspace_path: Uri = workspace_uri.parse().ok()?;
                                        let workspace_file_path = workspace_path.to_file_path()?;
                                        workspace_file_path.join(&loc.file_path)
                                    };

                                    Uri::from_file_path(file_path)?
                                };

                                // If the location is in the same file, adjust positions back to original file coordinates
                                let (start_line, start_char, end_line, end_char) = if file_uri
                                    == uri
                                {
                                    // Adjust positions back from extracted GraphQL to original file
                                    let adjusted_start_line =
                                        loc.range.start.line + block.start_line;
                                    let adjusted_start_char = if loc.range.start.line == 0 {
                                        loc.range.start.character + block.start_column
                                    } else {
                                        loc.range.start.character
                                    };
                                    let adjusted_end_line = loc.range.end.line + block.start_line;
                                    let adjusted_end_char = if loc.range.end.line == 0 {
                                        loc.range.end.character + block.start_column
                                    } else {
                                        loc.range.end.character
                                    };
                                    (
                                        adjusted_start_line as u32,
                                        adjusted_start_char as u32,
                                        adjusted_end_line as u32,
                                        adjusted_end_char as u32,
                                    )
                                } else {
                                    (
                                        loc.range.start.line as u32,
                                        loc.range.start.character as u32,
                                        loc.range.end.line as u32,
                                        loc.range.end.character as u32,
                                    )
                                };

                                Some(Location {
                                    uri: file_uri,
                                    range: Range {
                                        start: Position {
                                            line: start_line,
                                            character: start_char,
                                        },
                                        end: Position {
                                            line: end_line,
                                            character: end_char,
                                        },
                                    },
                                })
                            })
                            .collect();

                        if !lsp_locations.is_empty() {
                            return Ok(Some(GotoDefinitionResponse::Array(lsp_locations)));
                        }
                    }
                }
            } else {
                // Fallback: Extract GraphQL from TypeScript file (cache miss)
                tracing::debug!("Cache miss - extracting GraphQL from TypeScript file");
                let extract_config = project_guard.get_extract_config();
                let extracted =
                    match graphql_extract::extract_from_source(&content, language, &extract_config)
                    {
                        Ok(extracted) => extracted,
                        Err(e) => {
                            tracing::debug!(
                                "Failed to extract GraphQL from TypeScript file: {}",
                                e
                            );
                            return Ok(None);
                        }
                    };

                for item in extracted {
                    let start_line = item.location.range.start.line;
                    let end_line = item.location.range.end.line;

                    if cursor_line >= start_line && cursor_line <= end_line {
                        // Adjust position relative to the extracted GraphQL
                        #[allow(clippy::cast_possible_truncation)]
                        let relative_position = graphql_project::Position {
                            line: cursor_line - start_line,
                            character: if cursor_line == start_line {
                                lsp_position
                                    .character
                                    .saturating_sub(item.location.range.start.column as u32)
                                    as usize
                            } else {
                                lsp_position.character as usize
                            },
                        };

                        tracing::debug!(
                            "Adjusted position from {:?} to {:?} for extracted GraphQL",
                            lsp_position,
                            relative_position
                        );

                        // Get definition locations from the project using the extracted GraphQL
                        let Some(locations) = project_guard.goto_definition(
                            &item.source,
                            relative_position,
                            &uri.to_string(),
                        ) else {
                            tracing::debug!(
                                "No definition found at position {:?}",
                                relative_position
                            );
                            continue;
                        };

                        tracing::debug!("Found {} definition location(s)", locations.len());

                        // Convert to LSP Locations
                        #[allow(clippy::cast_possible_truncation)]
                        let lsp_locations: Vec<Location> = locations
                            .iter()
                            .filter_map(|loc| {
                                // Check if the file_path is already a URI
                                let file_uri = if loc.file_path.starts_with("file://") {
                                    // Already a URI, parse it directly
                                    loc.file_path.parse::<Uri>().ok()?
                                } else {
                                    // Resolve the file path relative to the workspace if it's not absolute
                                    let file_path = if std::path::Path::new(&loc.file_path)
                                        .is_absolute()
                                    {
                                        std::path::PathBuf::from(&loc.file_path)
                                    } else {
                                        // Resolve relative to workspace root
                                        let workspace_path: Uri = workspace_uri.parse().ok()?;
                                        let workspace_file_path = workspace_path.to_file_path()?;
                                        workspace_file_path.join(&loc.file_path)
                                    };
                                    Uri::from_file_path(file_path)?
                                };

                                // If the location is in the same file, adjust positions back to original file coordinates
                                let (start_line, start_char, end_line, end_char) = if file_uri
                                    == uri
                                {
                                    // Adjust positions back from extracted GraphQL to original file
                                    let adjusted_start_line = loc.range.start.line + start_line;
                                    let adjusted_start_char = if loc.range.start.line == 0 {
                                        loc.range.start.character + item.location.range.start.column
                                    } else {
                                        loc.range.start.character
                                    };
                                    let adjusted_end_line = loc.range.end.line + start_line;
                                    let adjusted_end_char = if loc.range.end.line == 0 {
                                        loc.range.end.character + item.location.range.start.column
                                    } else {
                                        loc.range.end.character
                                    };
                                    (
                                        adjusted_start_line as u32,
                                        adjusted_start_char as u32,
                                        adjusted_end_line as u32,
                                        adjusted_end_char as u32,
                                    )
                                } else {
                                    (
                                        loc.range.start.line as u32,
                                        loc.range.start.character as u32,
                                        loc.range.end.line as u32,
                                        loc.range.end.character as u32,
                                    )
                                };

                                Some(Location {
                                    uri: file_uri,
                                    range: Range {
                                        start: Position {
                                            line: start_line,
                                            character: start_char,
                                        },
                                        end: Position {
                                            line: end_line,
                                            character: end_char,
                                        },
                                    },
                                })
                            })
                            .collect();

                        if !lsp_locations.is_empty() {
                            return Ok(Some(GotoDefinitionResponse::Array(lsp_locations)));
                        }
                    }
                }
            }

            return Ok(None);
        }

        // For pure GraphQL files, use the content as-is
        let position = graphql_project::Position {
            line: lsp_position.line as usize,
            character: lsp_position.character as usize,
        };

        tracing::info!(
            "Calling project.goto_definition with position: {:?}",
            position
        );
        tracing::info!("Content length: {} bytes", content.len());

        // Get definition locations from the project
        let Some(locations) =
            project
                .read()
                .await
                .goto_definition(&content, position, &uri.to_string())
        else {
            tracing::info!(
                "project.goto_definition returned None at position {:?}",
                position
            );
            return Ok(None);
        };

        tracing::info!("Found {} definition location(s)", locations.len());
        for (idx, loc) in locations.iter().enumerate() {
            tracing::info!(
                "Location {}: file={}, line={}, col={}",
                idx,
                loc.file_path,
                loc.range.start.line,
                loc.range.start.character
            );
        }

        // Convert to LSP Locations
        #[allow(clippy::cast_possible_truncation)]
        let lsp_locations: Vec<Location> = locations
            .iter()
            .filter_map(|loc| {
                // Check if the file_path is already a URI
                let file_uri = if loc.file_path.starts_with("file://") {
                    // Already a URI, parse it directly
                    loc.file_path.parse::<Uri>().ok()?
                } else {
                    // Resolve the file path relative to the workspace if it's not absolute
                    let file_path = if std::path::Path::new(&loc.file_path).is_absolute() {
                        std::path::PathBuf::from(&loc.file_path)
                    } else {
                        // Resolve relative to workspace root
                        let workspace_path: Uri = workspace_uri.parse().ok()?;
                        let workspace_file_path = workspace_path.to_file_path()?;
                        workspace_file_path.join(&loc.file_path)
                    };

                    tracing::info!("Resolved file path: {:?}", file_path);
                    Uri::from_file_path(&file_path)?
                };

                tracing::info!("Created URI: {:?}", file_uri);

                let lsp_loc = Location {
                    uri: file_uri,
                    range: Range {
                        start: Position {
                            line: loc.range.start.line as u32,
                            character: loc.range.start.character as u32,
                        },
                        end: Position {
                            line: loc.range.end.line as u32,
                            character: loc.range.end.character as u32,
                        },
                    },
                };
                tracing::info!(
                    "LSP Location: uri={:?}, range={:?}",
                    lsp_loc.uri,
                    lsp_loc.range
                );
                Some(lsp_loc)
            })
            .collect();

        tracing::info!(
            "Goto definition completed in {:?}, returning {} location(s)",
            start.elapsed(),
            lsp_locations.len()
        );

        if lsp_locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(lsp_locations)))
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let start = std::time::Instant::now();
        let uri = params.text_document_position.text_document.uri;
        let lsp_position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        tracing::info!(
            "Find references requested: {:?} at line={} char={} (include_declaration: {})",
            uri,
            lsp_position.line,
            lsp_position.character,
            include_declaration
        );

        // Get the cached document content
        let Some(content) = self.document_cache.get(&uri.to_string()) else {
            tracing::warn!("No cached content for document: {:?}", uri);
            return Ok(None);
        };

        // Find the workspace and project for this document
        let Some((workspace_uri, project_idx)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get the project
        let Some(projects) = self.projects.get(&workspace_uri) else {
            tracing::warn!("No projects loaded for workspace: {workspace_uri}");
            return Ok(None);
        };

        let Some((_, project, _)) = projects.get(project_idx) else {
            tracing::warn!("Project index {project_idx} not found in workspace {workspace_uri}");
            return Ok(None);
        };

        // Collect all documents from the cache
        let collect_start = std::time::Instant::now();
        let all_documents: Vec<(String, String)> = self
            .document_cache
            .iter()
            .map(|entry| {
                let uri_string = entry.key().clone();
                let content = entry.value().clone();
                (uri_string, content)
            })
            .collect();

        tracing::info!(
            "Collected {} documents for reference search in {:?}",
            all_documents.len(),
            collect_start.elapsed()
        );

        // For find_references optimization, we would parse all documents once here
        // However, since the documents are already cached in document_index via did_open/did_change,
        // the actual optimization happens by reusing those cached ASTs.
        // We pass None here as the ASTs will be retrieved from document_index internally.
        let document_asts: Option<&std::collections::HashMap<String, graphql_project::SyntaxTree>> =
            None;

        // For pure GraphQL files (TypeScript extraction not implemented yet for find references)
        let position = graphql_project::Position {
            line: lsp_position.line as usize,
            character: lsp_position.character as usize,
        };

        // Find references with pre-parsed ASTs
        let find_start = std::time::Instant::now();
        let Some(references) = project.read().await.find_references_with_asts(
            &content,
            position,
            &all_documents,
            include_declaration,
            Some(&uri.to_string()),
            document_asts,
        ) else {
            tracing::info!("No references found at position {:?}", position);
            return Ok(None);
        };

        tracing::info!(
            "Found {} reference(s) in {:?}",
            references.len(),
            find_start.elapsed()
        );

        // Convert to LSP Locations
        #[allow(clippy::cast_possible_truncation)]
        let lsp_locations: Vec<Location> = references
            .iter()
            .filter_map(|reference_loc| {
                // The file_path in reference_loc is the URI string from the document cache
                // Parse it as a URI
                let file_uri: Uri = reference_loc.file_path.parse().ok()?;

                Some(Location {
                    uri: file_uri,
                    range: Range {
                        start: Position {
                            line: reference_loc.range.start.line as u32,
                            character: reference_loc.range.start.character as u32,
                        },
                        end: Position {
                            line: reference_loc.range.end.line as u32,
                            character: reference_loc.range.end.character as u32,
                        },
                    },
                })
            })
            .collect();

        tracing::info!(
            "Find references completed in {:?}, returning {} location(s)",
            start.elapsed(),
            lsp_locations.len()
        );

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
        tracing::debug!("Document symbols requested: {:?}", params.text_document.uri);
        // TODO: Implement document symbols
        Ok(None)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<OneOf<Vec<SymbolInformation>, Vec<WorkspaceSymbol>>>> {
        tracing::debug!("Workspace symbols requested: {}", params.query);
        // TODO: Implement workspace symbols
        Ok(None)
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

            // Collect status information
            for workspace_entry in self.workspace_roots.iter() {
                let workspace_uri = workspace_entry.key();
                let workspace_path = workspace_entry.value();

                status_lines.push(format!("Workspace: {}", workspace_path.display()));

                // Get config path
                if let Some(config_path) = self.config_paths.get(workspace_uri) {
                    status_lines.push(format!(
                        "  Config: {}",
                        config_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                    ));
                }

                // Get projects for this workspace
                if let Some(projects) = self.projects.get(workspace_uri) {
                    for (project_name, project, _) in projects.iter() {
                        let project_guard = project.read().await;

                        // Schema information
                        let schema_index = project_guard.schema_index();
                        if let Ok(schema_guard) = schema_index.read() {
                            let schema = schema_guard.schema();
                            let type_count = schema.types.len();

                            // Count fields from object and interface types
                            let field_count: usize = schema
                                .types
                                .values()
                                .filter_map(|type_def| {
                                    if let Some(obj) = type_def.as_object() {
                                        Some(obj.fields.len())
                                    } else if let Some(iface) = type_def.as_interface() {
                                        Some(iface.fields.len())
                                    } else {
                                        None
                                    }
                                })
                                .sum();

                            status_lines.push(format!(
                                "  Project '{}': {} types, {} fields",
                                project_name, type_count, field_count
                            ));
                        } else {
                            status_lines.push(format!(
                                "  Project '{}': ⚠️ Schema not loaded",
                                project_name
                            ));
                        }

                        // Document information
                        {
                            let document_index = project_guard.document_index();
                            if let Ok(doc_guard) = document_index.read() {
                                let operation_count = doc_guard.operations.len();
                                let fragment_count = doc_guard.fragments.len();

                                status_lines.push(format!(
                                    "    {} operations, {} fragments",
                                    operation_count, fragment_count
                                ));
                            } else {
                                status_lines.push("    ⚠️ Documents not loaded".to_string());
                            };
                        }
                    }
                } else {
                    status_lines.push("  ⚠️ No projects loaded".to_string());
                }
            }

            // Open documents
            status_lines.push("".to_string());
            status_lines.push(format!(
                "{} files open in editor",
                self.document_cache.len()
            ));

            let status_report = status_lines.join("\n");

            // Log detailed status to both tracing and LSP output
            let full_report = format!("\n=== GraphQL LSP Status ===\n{}\n", status_report);
            tracing::info!("{}", full_report);

            self.client
                .log_message(MessageType::INFO, full_report)
                .await;

            // Show a simple notification
            let summary = if self.workspace_roots.is_empty() {
                "No workspaces loaded".to_string()
            } else {
                let workspace_count = self.workspace_roots.len();
                let project_count: usize = self.projects.iter().map(|p| p.value().len()).sum();
                format!(
                    "{} workspace(s), {} project(s) - Check output for details",
                    workspace_count, project_count
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
