use dashmap::DashMap;
use graphql_config::{find_config, load_config};
use graphql_extract::ExtractConfig;
use graphql_project::GraphQLProject;
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, Location, MessageType, OneOf,
    Position, Range, ReferenceParams, ServerCapabilities, ServerInfo, SymbolInformation,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkspaceSymbol,
    WorkspaceSymbolParams,
};
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::{Client, LanguageServer};

pub struct GraphQLLanguageServer {
    client: Client,
    /// Workspace folders from initialization (stored temporarily until we load configs)
    init_workspace_folders: Arc<DashMap<String, PathBuf>>,
    /// Workspace roots indexed by workspace folder URI string
    workspace_roots: Arc<DashMap<String, PathBuf>>,
    /// GraphQL projects by workspace URI -> Vec<(project_name, project)>
    projects: Arc<DashMap<String, Vec<(String, GraphQLProject)>>>,
}

impl GraphQLLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            init_workspace_folders: Arc::new(DashMap::new()),
            workspace_roots: Arc::new(DashMap::new()),
            projects: Arc::new(DashMap::new()),
        }
    }

    /// Load GraphQL config from a workspace folder
    async fn load_workspace_config(&self, workspace_uri: &str, workspace_path: &PathBuf) {
        tracing::info!("Loading GraphQL config from {:?}", workspace_path);

        // Find graphql config
        match find_config(workspace_path) {
            Ok(Some(config_path)) => {
                tracing::info!("Found GraphQL config at {:?}", config_path);

                // Load the config
                match load_config(&config_path) {
                    Ok(config) => {
                        // Create projects from config
                        match GraphQLProject::from_config_with_base(&config, workspace_path) {
                            Ok(mut projects) => {
                                tracing::info!("Loaded {} GraphQL project(s)", projects.len());

                                // Load schemas for all projects
                                for (name, project) in &projects {
                                    if let Err(e) = project.load_schema().await {
                                        tracing::error!("Failed to load schema for project '{}': {}", name, e);
                                        self.client
                                            .log_message(
                                                MessageType::ERROR,
                                                format!("Failed to load schema for project '{}': {}", name, e),
                                            )
                                            .await;
                                    } else {
                                        tracing::info!("Loaded schema for project '{}'", name);
                                    }
                                }

                                // Store workspace and projects
                                self.workspace_roots
                                    .insert(workspace_uri.to_string(), workspace_path.clone());
                                self.projects.insert(workspace_uri.to_string(), projects);

                                self.client
                                    .log_message(MessageType::INFO, "GraphQL config loaded successfully")
                                    .await;
                            }
                            Err(e) => {
                                tracing::error!("Failed to create projects from config: {}", e);
                                self.client
                                    .log_message(
                                        MessageType::ERROR,
                                        format!("Failed to load GraphQL projects: {}", e),
                                    )
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to load config: {}", e);
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to parse GraphQL config: {}", e),
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

    /// Find the project for a given document URI
    fn find_project_for_document(&self, document_uri: &Uri) -> Option<GraphQLProject> {
        let doc_path = document_uri.path();

        // Try to find which workspace this document belongs to
        for workspace_entry in self.workspace_roots.iter() {
            let workspace_uri = workspace_entry.key();
            let workspace_path = workspace_entry.value();

            if doc_path.starts_with(workspace_path.to_str()?) {
                // Found the workspace, now get the first project
                // TODO: Match document to correct project based on includes/excludes
                if let Some(projects) = self.projects.get(workspace_uri) {
                    if let Some((_, project)) = projects.first() {
                        return Some(project.clone());
                    }
                }
            }
        }

        None
    }

    /// Validate a document and publish diagnostics
    async fn validate_document(&self, uri: Uri, content: &str) {
        let Some(project) = self.find_project_for_document(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return;
        };

        // Check if this is a TypeScript/JavaScript file
        let is_ts_js = uri.path().ends_with(".ts")
            || uri.path().ends_with(".tsx")
            || uri.path().ends_with(".js")
            || uri.path().ends_with(".jsx");

        let diagnostics = if is_ts_js {
            self.validate_typescript_document(&uri, content, &project)
        } else {
            self.validate_graphql_document(content, &project)
        };

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }

    /// Validate a pure GraphQL document
    fn validate_graphql_document(&self, content: &str, project: &GraphQLProject) -> Vec<Diagnostic> {
        match project.validate_document(content) {
            Ok(_) => vec![],
            Err(diagnostic_list) => self.convert_diagnostics(&diagnostic_list),
        }
    }

    /// Validate GraphQL embedded in TypeScript/JavaScript
    fn validate_typescript_document(
        &self,
        uri: &Uri,
        content: &str,
        project: &GraphQLProject,
    ) -> Vec<Diagnostic> {
        // Write content to a temp file for extraction
        // graphql-extract needs a file path to parse
        use std::io::Write;
        let temp_file = match tempfile::NamedTempFile::new() {
            Ok(mut file) => {
                if file.write_all(content.as_bytes()).is_err() {
                    return vec![];
                }
                file
            }
            Err(_) => return vec![],
        };

        // Extract GraphQL from TypeScript/JavaScript
        let extracted = match graphql_extract::extract_from_file(
            temp_file.path(),
            &ExtractConfig::default(),
        ) {
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

        let mut all_diagnostics = Vec::new();

        // Validate each extracted document
        for item in extracted {
            let line_offset = item.location.range.start.line;

            match project.validate_document_with_location(&item.source, &uri.to_string(), line_offset) {
                Ok(_) => {}
                Err(diagnostic_list) => {
                    all_diagnostics.extend(self.convert_diagnostics(&diagnostic_list));
                }
            }
        }

        all_diagnostics
    }

    /// Convert apollo-compiler diagnostics to LSP diagnostics
    fn convert_diagnostics(
        &self,
        diagnostic_list: &apollo_compiler::validation::DiagnosticList,
    ) -> Vec<Diagnostic> {
        diagnostic_list
            .iter()
            .filter_map(|diag| {
                let range = if let Some(loc_range) = diag.line_column_range() {
                    // apollo-compiler uses 1-based, LSP uses 0-based
                    Range {
                        start: Position {
                            line: loc_range.start.line.saturating_sub(1) as u32,
                            character: loc_range.start.column.saturating_sub(1) as u32,
                        },
                        end: Position {
                            line: loc_range.end.line.saturating_sub(1) as u32,
                            character: loc_range.end.column.saturating_sub(1) as u32,
                        },
                    }
                } else {
                    Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 1,
                        },
                    }
                };

                Some(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    source: Some("graphql".to_string()),
                    message: format!("{}", diag.error),
                    ..Default::default()
                })
            })
            .collect()
    }
}

impl LanguageServer for GraphQLLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing GraphQL Language Server");

        // Store workspace folders for later config loading
        if let Some(ref folders) = params.workspace_folders {
            tracing::info!("Workspace folders: {} folders", folders.len());
            for folder in folders {
                if let Ok(path) = folder.uri.to_file_path() {
                    self.init_workspace_folders
                        .insert(folder.uri.to_string(), path);
                }
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "{".to_string(),
                        "@".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "GraphQL Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, params: InitializedParams) {
        tracing::info!("GraphQL Language Server initialized");
        self.client
            .log_message(MessageType::INFO, "GraphQL LSP initialized")
            .await;

        // Load GraphQL config from all workspace folders
        // We need to get workspace folders from somewhere - let's check if we saved them during initialize
        // For now, we'll need to pass them through initialization_options or wait for a workspace folder change
        // Let me check the InitializeParams we got in initialize()
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down GraphQL Language Server");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        tracing::info!("Document opened: {:?}", uri);

        // For now, use a simple schema. Later this should load from graphql.config
        // You can create a test schema here or load from workspace
        let schema_content = r#"
            type Query {
                user(id: ID!): User
                post(id: ID!): Post
            }

            type User {
                id: ID!
                name: String!
                posts: [Post!]!
            }

            type Post {
                id: ID!
                title: String!
                content: String!
                author: User!
            }
        "#;

        let schema = SchemaIndex::from_schema(schema_content);

        // Store document state
        self.documents.insert(
            uri.clone(),
            DocumentState {
                content: content.clone(),
                schema: Some(schema.clone()),
            },
        );

        // Validate and publish diagnostics
        self.validate_and_publish(uri, &content, &schema).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        tracing::debug!("Document changed: {:?}", uri);

        // Get the latest content from the changes
        for change in params.content_changes {
            if let Some(mut doc_state) = self.documents.get_mut(&uri) {
                // For full sync, replace entire document
                doc_state.content = change.text.clone();

                // Validate if we have a schema
                if let Some(schema) = &doc_state.schema {
                    let content = doc_state.content.clone();
                    let schema = schema.clone();
                    drop(doc_state); // Release the lock before async call

                    self.validate_and_publish(uri.clone(), &content, &schema)
                        .await;
                }
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        tracing::info!("Document saved: {:?}", params.text_document.uri);
        // TODO: Re-validate document
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        tracing::info!("Document closed: {:?}", params.text_document.uri);
        // TODO: Clean up document state
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        tracing::debug!(
            "Completion requested: {:?}",
            params.text_document_position.text_document.uri
        );
        // TODO: Implement autocompletion
        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        tracing::debug!(
            "Hover requested: {:?}",
            params.text_document_position_params.text_document.uri
        );
        // TODO: Implement hover information
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        tracing::debug!(
            "Go to definition requested: {:?}",
            params.text_document_position_params.text_document.uri
        );
        // TODO: Implement go-to-definition
        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        tracing::debug!(
            "References requested: {:?}",
            params.text_document_position.text_document.uri
        );
        // TODO: Implement find references
        Ok(None)
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
}
