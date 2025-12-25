// Allow nursery clippy lints that are too pedantic for our use case
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::significant_drop_in_scrutinee)]
#![allow(dead_code)] // Temporary - during Phase 6 cleanup

use dashmap::DashMap;
use graphql_config::find_config;
use graphql_ide::AnalysisHost;
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FileChangeType,
    FileSystemWatcher, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, Location,
    MessageType, OneOf, Position, Range, ReferenceParams, ServerCapabilities, ServerInfo,
    SymbolInformation, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    WorkDoneProgressOptions, WorkspaceSymbol, WorkspaceSymbolParams,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::{Client, LanguageServer, UriExt};

// ============================================================================
// Type Conversion Functions (LSP ↔ graphql-ide)
// ============================================================================

// Allow dead code temporarily - these will be used as we migrate handlers
#[allow(dead_code)]
/// Convert LSP Position to graphql-ide Position
const fn convert_lsp_position(pos: Position) -> graphql_ide::Position {
    graphql_ide::Position::new(pos.line, pos.character)
}

#[allow(dead_code)]
/// Convert graphql-ide Position to LSP Position
const fn convert_ide_position(pos: graphql_ide::Position) -> Position {
    Position {
        line: pos.line,
        character: pos.character,
    }
}

#[allow(dead_code)]
/// Convert graphql-ide Range to LSP Range
const fn convert_ide_range(range: graphql_ide::Range) -> Range {
    Range {
        start: convert_ide_position(range.start),
        end: convert_ide_position(range.end),
    }
}

#[allow(dead_code)]
/// Convert graphql-ide Location to LSP Location
fn convert_ide_location(loc: &graphql_ide::Location) -> Location {
    Location {
        uri: loc.file.as_str().parse().expect("Invalid URI"),
        range: convert_ide_range(loc.range),
    }
}

#[allow(dead_code)]
/// Convert graphql-ide `CompletionItem` to LSP `CompletionItem`
fn convert_ide_completion_item(item: graphql_ide::CompletionItem) -> lsp_types::CompletionItem {
    lsp_types::CompletionItem {
        label: item.label,
        kind: Some(match item.kind {
            graphql_ide::CompletionKind::Field => lsp_types::CompletionItemKind::FIELD,
            graphql_ide::CompletionKind::Type => lsp_types::CompletionItemKind::CLASS,
            graphql_ide::CompletionKind::Fragment => lsp_types::CompletionItemKind::SNIPPET,
            graphql_ide::CompletionKind::Directive => lsp_types::CompletionItemKind::KEYWORD,
            graphql_ide::CompletionKind::EnumValue => lsp_types::CompletionItemKind::ENUM_MEMBER,
            graphql_ide::CompletionKind::Argument => lsp_types::CompletionItemKind::PROPERTY,
            graphql_ide::CompletionKind::Variable => lsp_types::CompletionItemKind::VARIABLE,
        }),
        detail: item.detail,
        documentation: item.documentation.map(|doc| {
            lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: doc,
            })
        }),
        deprecated: Some(item.deprecated),
        insert_text: item.insert_text,
        ..Default::default()
    }
}

#[allow(dead_code)]
/// Convert graphql-ide `HoverResult` to LSP Hover
fn convert_ide_hover(hover: graphql_ide::HoverResult) -> Hover {
    Hover {
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: hover.contents,
        }),
        range: hover.range.map(convert_ide_range),
    }
}

// Removed: load_lsp_lint_config_for_project - old project system

pub struct GraphQLLanguageServer {
    client: Client,
    /// Workspace folders from initialization (stored temporarily until we load configs)
    init_workspace_folders: Arc<DashMap<String, PathBuf>>,
    /// Workspace roots indexed by workspace folder URI string
    workspace_roots: Arc<DashMap<String, PathBuf>>,
    /// Config file paths indexed by workspace URI string
    config_paths: Arc<DashMap<String, PathBuf>>,
    /// `AnalysisHost` per workspace (workspace URI -> `AnalysisHost`)
    hosts: Arc<DashMap<String, Arc<Mutex<AnalysisHost>>>>,
}

impl GraphQLLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            init_workspace_folders: Arc::new(DashMap::new()),
            workspace_roots: Arc::new(DashMap::new()),
            config_paths: Arc::new(DashMap::new()),
            hosts: Arc::new(DashMap::new()),
        }
    }

    /// Get or create an `AnalysisHost` for a workspace
    fn get_or_create_host(&self, workspace_uri: &str) -> Arc<Mutex<AnalysisHost>> {
        self.hosts
            .entry(workspace_uri.to_string())
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
        // Determine kind based on file extension
        if path.ends_with(".ts") || path.ends_with(".tsx") {
            graphql_ide::FileKind::TypeScript
        } else if path.ends_with(".js") || path.ends_with(".jsx") {
            graphql_ide::FileKind::JavaScript
        } else {
            // .graphql, .gql, or other files from documents pattern
            graphql_ide::FileKind::ExecutableGraphQL
        }
    }

    /// Determine if a file contains schema definitions by inspecting its content.
    ///
    /// Used for files opened/changed in the editor where we don't have config context.
    /// Returns true if the content contains schema type definitions.
    fn content_has_schema_definitions(content: &str) -> bool {
        use apollo_compiler::parser::Parser;

        let mut parser = Parser::new();
        let ast = parser
            .parse_ast(content, "virtual.graphql")
            .unwrap_or_else(|e| e.partial);

        ast.definitions.iter().any(|def| {
            matches!(
                def,
                apollo_compiler::ast::Definition::SchemaDefinition(_)
                    | apollo_compiler::ast::Definition::SchemaExtension(_)
                    | apollo_compiler::ast::Definition::ObjectTypeDefinition(_)
                    | apollo_compiler::ast::Definition::ObjectTypeExtension(_)
                    | apollo_compiler::ast::Definition::InterfaceTypeDefinition(_)
                    | apollo_compiler::ast::Definition::InterfaceTypeExtension(_)
                    | apollo_compiler::ast::Definition::UnionTypeDefinition(_)
                    | apollo_compiler::ast::Definition::UnionTypeExtension(_)
                    | apollo_compiler::ast::Definition::ScalarTypeDefinition(_)
                    | apollo_compiler::ast::Definition::ScalarTypeExtension(_)
                    | apollo_compiler::ast::Definition::EnumTypeDefinition(_)
                    | apollo_compiler::ast::Definition::EnumTypeExtension(_)
                    | apollo_compiler::ast::Definition::InputObjectTypeDefinition(_)
                    | apollo_compiler::ast::Definition::InputObjectTypeExtension(_)
                    | apollo_compiler::ast::Definition::DirectiveDefinition(_)
            )
        })
    }

    /// Determine `FileKind` for files opened/changed in the editor.
    ///
    /// For TypeScript/JavaScript files, we can't easily distinguish between schema
    /// and documents without config context, so we check the extracted GraphQL content.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn determine_file_kind_from_content(path: &str, content: &str) -> graphql_ide::FileKind {
        // For TypeScript/JavaScript, check if content has schema definitions
        if path.ends_with(".ts") || path.ends_with(".tsx") {
            return graphql_ide::FileKind::TypeScript;
        }
        if path.ends_with(".js") || path.ends_with(".jsx") {
            return graphql_ide::FileKind::JavaScript;
        }

        // For .graphql/.gql files, check content to determine Schema vs ExecutableGraphQL
        if Self::content_has_schema_definitions(content) {
            graphql_ide::FileKind::Schema
        } else {
            graphql_ide::FileKind::ExecutableGraphQL
        }
    }

    /// Extract GraphQL from TypeScript/JavaScript source code.
    ///
    /// Returns `(extracted_graphql, line_offset)` tuple.
    /// For TS/JS files: Returns extracted GraphQL or empty string if none found.
    /// For other files: Returns source as-is.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn extract_graphql_from_source(
        path: &str,
        source: &str,
        config: &graphql_extract::ExtractConfig,
    ) -> (String, u32) {
        use graphql_extract::{extract_from_source, Language};

        // Determine language from file extension
        let language = if path.ends_with(".ts") || path.ends_with(".tsx") {
            Language::TypeScript
        } else if path.ends_with(".js") || path.ends_with(".jsx") {
            Language::JavaScript
        } else {
            // Not a TS/JS file, return as-is (for .graphql files)
            return (source.to_string(), 0);
        };

        // Extract GraphQL from TS/JS using provided config
        tracing::info!("Attempting to extract GraphQL from TS/JS file: {}", path);
        match extract_from_source(source, language, config) {
            Ok(extracted) if !extracted.is_empty() => {
                // Concatenate all extracted GraphQL blocks
                let combined_graphql: Vec<String> =
                    extracted.iter().map(|e| e.source.clone()).collect();

                // Use the line offset from the first block (already 0-indexed)
                #[allow(clippy::cast_possible_truncation)]
                let line_offset = extracted[0].location.range.start.line as u32;

                let result = combined_graphql.join("\n\n");
                tracing::info!(
                    "Successfully extracted GraphQL from {}: {} blocks, {} chars, line_offset={}",
                    path,
                    extracted.len(),
                    result.len(),
                    line_offset
                );
                (result, line_offset)
            }
            Ok(extracted) => {
                // No GraphQL found in TS/JS file - return empty string
                tracing::warn!(
                    "No GraphQL found in TypeScript/JavaScript file: {} (extracted {} blocks)",
                    path,
                    extracted.len()
                );
                (String::new(), 0)
            }
            Err(e) => {
                // Extraction failed - return empty string
                tracing::error!("Failed to extract GraphQL from {}: {}", path, e);
                (String::new(), 0)
            }
        }
    }

    /// Expand brace patterns like `{ts,tsx}` into multiple patterns
    ///
    /// This is needed because the glob crate doesn't support brace expansion.
    /// For example, `**/*.{ts,tsx}` expands to `["**/*.ts", "**/*.tsx"]`.
    fn expand_braces(pattern: &str) -> Vec<String> {
        // Simple brace expansion for patterns like **/*.{ts,tsx}
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

    /// Extract fragment names defined in GraphQL content
    fn extract_fragment_names_from_content(content: &str) -> std::collections::HashSet<String> {
        use apollo_parser::{cst, Parser};
        use std::collections::HashSet;

        let mut fragment_names = HashSet::new();
        let parser = Parser::new(content);
        let tree = parser.parse();

        for definition in tree.document().definitions() {
            if let cst::Definition::FragmentDefinition(fragment) = definition {
                if let Some(name) = fragment.fragment_name() {
                    if let Some(name_token) = name.name() {
                        fragment_names.insert(name_token.text().to_string());
                    }
                }
            }
        }

        fragment_names
    }

    /// Check if a document references any of the given fragments (transitively)
    fn document_references_fragments(
        content: &str,
        fragment_names: &std::collections::HashSet<String>,
    ) -> bool {
        use apollo_parser::{cst, Parser};
        use std::collections::VecDeque;

        let parser = Parser::new(content);
        let tree = parser.parse();

        // Collect all directly referenced fragments from operations and fragment definitions
        let mut to_process = VecDeque::new();

        for definition in tree.document().definitions() {
            match definition {
                cst::Definition::OperationDefinition(operation) => {
                    if let Some(selection_set) = operation.selection_set() {
                        Self::collect_fragment_spreads_recursive(&selection_set, &mut to_process);
                    }
                }
                cst::Definition::FragmentDefinition(fragment) => {
                    if let Some(selection_set) = fragment.selection_set() {
                        Self::collect_fragment_spreads_recursive(&selection_set, &mut to_process);
                    }
                }
                _ => {}
            }
        }

        // Check if any of the referenced fragments match the changed fragments
        while let Some(frag_name) = to_process.pop_front() {
            if fragment_names.contains(&frag_name) {
                return true;
            }
        }

        // TODO: Could also check transitive references by looking up fragments
        // in the document index, but direct references are sufficient for now

        false
    }

    /// Recursively collect fragment spread names from a selection set
    fn collect_fragment_spreads_recursive(
        selection_set: &apollo_parser::cst::SelectionSet,
        result: &mut std::collections::VecDeque<String>,
    ) {
        for selection in selection_set.selections() {
            match selection {
                apollo_parser::cst::Selection::FragmentSpread(spread) => {
                    if let Some(name) = spread.fragment_name() {
                        if let Some(name_token) = name.name() {
                            result.push_back(name_token.text().to_string());
                        }
                    }
                }
                apollo_parser::cst::Selection::Field(field) => {
                    if let Some(nested_selection_set) = field.selection_set() {
                        Self::collect_fragment_spreads_recursive(&nested_selection_set, result);
                    }
                }
                apollo_parser::cst::Selection::InlineFragment(inline) => {
                    if let Some(nested_selection_set) = inline.selection_set() {
                        Self::collect_fragment_spreads_recursive(&nested_selection_set, result);
                    }
                }
            }
        }
    }

    /// Add or update a file in the `AnalysisHost` for a workspace
    async fn add_file_to_host(
        &self,
        workspace_uri: &str,
        file_uri: &Uri,
        content: &str,
        file_kind: graphql_ide::FileKind,
        line_offset: u32,
    ) {
        let host = self.get_or_create_host(workspace_uri);
        let file_path = graphql_ide::FilePath::new(file_uri.to_string());

        let mut host_guard = host.lock().await;
        host_guard.add_file(&file_path, content, file_kind, line_offset);
    }

    /// Remove a file from the `AnalysisHost` for a workspace
    async fn remove_file_from_host(&self, workspace_uri: &str, file_uri: &Uri) {
        if let Some(host) = self.hosts.get(workspace_uri) {
            let file_path = graphql_ide::FilePath::new(file_uri.to_string());
            let mut host_guard = host.lock().await;
            host_guard.remove_file(&file_path);
        }
    }

    /// Load schema files from a project into the `AnalysisHost`
    // TODO: Implement load_schema_into_host without old project
    #[allow(clippy::unused_self)]
    #[allow(clippy::missing_const_for_fn)]
    fn load_schema_into_host(&self, _workspace_uri: &str) {
        // Placeholder - schema loading happens via did_open
    }

    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    /// Load GraphQL config from a workspace folder and load all project files
    async fn load_workspace_config(&self, workspace_uri: &str, workspace_path: &PathBuf) {
        tracing::info!(path = ?workspace_path, "Loading GraphQL config");

        // Store workspace root
        self.workspace_roots
            .insert(workspace_uri.to_string(), workspace_path.clone());

        // Find and load config
        match find_config(workspace_path) {
            Ok(Some(config_path)) => {
                self.config_paths
                    .insert(workspace_uri.to_string(), config_path.clone());

                // Parse the config to load all files
                match graphql_config::load_config(&config_path) {
                    Ok(config) => {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                "GraphQL config found, loading files...",
                            )
                            .await;

                        // Load all files from all projects into AnalysisHost
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

    /// Load all GraphQL files from the config into `AnalysisHost`
    #[allow(clippy::too_many_lines)]
    async fn load_all_project_files(
        &self,
        workspace_uri: &str,
        workspace_path: &PathBuf,
        config: &graphql_config::GraphQLConfig,
    ) {
        use graphql_project::SchemaLoader;

        let host = self.get_or_create_host(workspace_uri);

        // Collect projects into a Vec to avoid holding iterator across await
        let projects: Vec<_> = config.projects().collect();

        for (_project_name, project_config) in projects {
            // Parse and set extract configuration
            let extract_config = project_config
                .extensions
                .as_ref()
                .and_then(|extensions| extensions.get("extractConfig"))
                .and_then(|extract_config_value| {
                    match serde_json::from_value::<graphql_extract::ExtractConfig>(
                        extract_config_value.clone(),
                    ) {
                        Ok(config) => {
                            tracing::info!("Loaded extract configuration from project config");
                            Some(config)
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse extract configuration: {e}, using defaults"
                            );
                            None
                        }
                    }
                })
                .unwrap_or_default();

            // Set extract config on host
            {
                let mut host_guard = host.lock().await;
                host_guard.set_extract_config(extract_config.clone());
            }

            // Load schema files using SchemaLoader
            let mut schema_loader = SchemaLoader::new(project_config.schema.clone());
            schema_loader = schema_loader.with_base_path(workspace_path);

            match schema_loader.load_with_paths().await {
                Ok(schema_files) => {
                    tracing::info!("Loading {} schema files", schema_files.len());
                    for (path, content) in schema_files {
                        // Files from the schema path are always Schema files
                        let file_kind = graphql_ide::FileKind::Schema;

                        // Extract GraphQL from TypeScript/JavaScript schema files
                        let (final_content, line_offset) =
                            Self::extract_graphql_from_source(&path, &content, &extract_config);

                        // Strip leading '/' from absolute paths to avoid file:////
                        let path_str = path.trim_start_matches('/');
                        let uri = format!("file:///{path_str}");
                        let file_path = graphql_ide::FilePath::new(uri);

                        let mut host_guard = host.lock().await;
                        host_guard.add_file(&file_path, &final_content, file_kind, line_offset);
                        tracing::info!("Loaded schema file: {}", path);
                    }
                }
                Err(e) => {
                    tracing::error!("Error loading schema files: {}", e);
                }
            }

            // Load document files (operations and fragments)
            if let Some(documents_config) = &project_config.documents {
                // Get document patterns from config
                let patterns: Vec<String> = documents_config
                    .patterns()
                    .into_iter()
                    .map(std::string::ToString::to_string)
                    .collect();

                tracing::info!("Loading document files with {} patterns", patterns.len());

                for pattern in patterns {
                    // Skip negation patterns (starting with !)
                    if pattern.trim().starts_with('!') {
                        tracing::info!("Skipping negation pattern: {}", pattern);
                        continue;
                    }

                    tracing::info!("Processing pattern: {}", pattern);

                    // Expand brace patterns like {ts,tsx} since glob crate doesn't support them
                    let expanded_patterns = Self::expand_braces(&pattern);
                    tracing::info!("Expanded into {} patterns", expanded_patterns.len());

                    for expanded_pattern in expanded_patterns {
                        // Resolve pattern relative to workspace
                        let full_pattern = workspace_path.join(&expanded_pattern);

                        tracing::debug!("Globbing: {}", full_pattern.display());

                        match glob::glob(&full_pattern.display().to_string()) {
                            Ok(paths) => {
                                for entry in paths {
                                    match entry {
                                        Ok(path) if path.is_file() => {
                                            // Skip node_modules
                                            if path
                                                .components()
                                                .any(|c| c.as_os_str() == "node_modules")
                                            {
                                                continue;
                                            }

                                            // Read file content
                                            match std::fs::read_to_string(&path) {
                                                Ok(content) => {
                                                    let path_str = path.display().to_string();
                                                    let file_kind = Self::determine_file_kind(
                                                        &path_str, &content,
                                                    );

                                                    // Extract GraphQL from TypeScript/JavaScript files
                                                    let (final_content, line_offset) =
                                                        Self::extract_graphql_from_source(
                                                            &path_str,
                                                            &content,
                                                            &extract_config,
                                                        );

                                                    // IMPORTANT: After extraction, change TypeScript/JavaScript to ExecutableGraphQL
                                                    let final_kind = match file_kind {
                                                        graphql_ide::FileKind::TypeScript
                                                        | graphql_ide::FileKind::JavaScript
                                                            if !final_content.is_empty() =>
                                                        {
                                                            graphql_ide::FileKind::ExecutableGraphQL
                                                        }
                                                        _ => file_kind,
                                                    };

                                                    // Strip leading '/' from absolute paths to avoid file:////
                                                    let path_str = path_str.trim_start_matches('/');
                                                    let uri = format!("file:///{path_str}");
                                                    let file_path = graphql_ide::FilePath::new(uri);

                                                    let mut host_guard = host.lock().await;
                                                    host_guard.add_file(
                                                        &file_path,
                                                        &final_content,
                                                        final_kind,
                                                        line_offset,
                                                    );
                                                    tracing::info!(
                                                        "Loaded document file: {}",
                                                        path.display()
                                                    );
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
                                        Ok(_) => {} // Skip directories
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
            }
        }

        tracing::info!("Finished loading all project files into AnalysisHost");
    }

    /// Reload GraphQL config for a workspace
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(workspace_uri = %workspace_uri))]
    // REMOVED: reload_workspace_config (old validation system)
    async fn reload_workspace_config(&self, workspace_uri: &str) {}

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
    // REMOVED: revalidate_all_documents (old validation system)
    #[allow(clippy::unused_self)]
    #[allow(clippy::missing_const_for_fn)]
    fn revalidate_all_documents(&self) {}

    /// Re-validate schema files when field usage changes in a document
    ///
    /// When operations or fragments change, the set of used fields changes,
    /// which affects `unused_fields` diagnostics in schema files. This method
    /// finds all schema files and re-publishes their diagnostics.
    // REMOVED: revalidate_schema_files (old validation system)
    #[allow(clippy::unused_self)]
    #[allow(clippy::missing_const_for_fn)]
    fn revalidate_schema_files(&self, _changed_uri: &Uri) {}

    /// Validate a document and publish diagnostics
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(path = ?uri.to_file_path().unwrap()))]
    async fn validate_document(&self, uri: Uri) {
        tracing::debug!("Starting document validation");

        // Find the workspace for this document
        let Some((workspace_uri, _project_idx)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No workspace found for document");
            return;
        };

        // Get the analysis host for this workspace
        let Some(host_mutex) = self.hosts.get(&workspace_uri) else {
            tracing::warn!("No analysis host found for workspace");
            return;
        };
        let host = host_mutex.lock().await;

        // Get the file path
        let file_path = graphql_ide::FilePath::new(uri.as_str());

        // Get diagnostics from the IDE layer
        let snapshot = host.snapshot();
        let diagnostics = snapshot.diagnostics(&file_path);
        tracing::debug!(
            diagnostic_count = diagnostics.len(),
            "Got diagnostics from IDE layer"
        );

        // Convert IDE diagnostics to LSP diagnostics
        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();

        // Publish diagnostics
        self.client
            .publish_diagnostics(uri, lsp_diagnostics, None)
            .await;

        tracing::debug!("Published diagnostics");
    }
}

/// Convert graphql-ide diagnostic to LSP diagnostic
fn convert_ide_diagnostic(diag: graphql_ide::Diagnostic) -> Diagnostic {
    let severity = match diag.severity {
        graphql_ide::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        graphql_ide::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        graphql_ide::DiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        graphql_ide::DiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    };

    Diagnostic {
        range: Range {
            start: Position {
                line: diag.range.start.line,
                character: diag.range.start.character,
            },
            end: Position {
                line: diag.range.end.line,
                character: diag.range.end.character,
            },
        },
        severity: Some(severity),
        code: diag.code.map(lsp_types::NumberOrString::String),
        source: Some(diag.source),
        message: diag.message,
        ..Default::default()
    }
}

impl GraphQLLanguageServer {
    // REMOVED: get_project_wide_diagnostics (old validation system)
    // REMOVED: refresh_affected_files_diagnostics (old validation system)
    // REMOVED: validate_graphql_document (old validation)
    // REMOVED: validate_typescript_document (old validation)

    /// Convert graphql-project diagnostic to LSP diagnostic
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

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        tracing::info!(
            content_len = content.len(),
            first_100 = ?&content[..100.min(content.len())],
            "Document opened"
        );

        // Add to AnalysisHost
        if let Some((workspace_uri, _project_idx)) = self.find_workspace_and_project(&uri) {
            let _file_path = uri.to_file_path();

            // Get extract config from host
            let extract_config = if let Some(host_mutex) = self.hosts.get(&workspace_uri) {
                let host = host_mutex.lock().await;
                host.get_extract_config()
            } else {
                graphql_extract::ExtractConfig::default()
            };

            // Determine file kind by inspecting path and content
            let file_kind = Self::determine_file_kind_from_content(uri.path().as_str(), &content);
            tracing::info!("Determined file_kind: {:?}", file_kind);

            // Extract GraphQL from TypeScript/JavaScript files
            tracing::info!("About to extract GraphQL, path={}", uri.path().as_str());
            let (final_content, line_offset) =
                Self::extract_graphql_from_source(uri.path().as_str(), &content, &extract_config);
            tracing::info!(
                extracted_len = final_content.len(),
                line_offset = line_offset,
                first_100 = ?&final_content[..100.min(final_content.len())],
                "Extracted GraphQL content"
            );

            // IMPORTANT: After extraction, the content is pure GraphQL, so change the kind to ExecutableGraphQL
            // This prevents the syntax layer from trying to extract again
            let final_kind = match file_kind {
                graphql_ide::FileKind::TypeScript | graphql_ide::FileKind::JavaScript
                    if !final_content.is_empty() =>
                {
                    graphql_ide::FileKind::ExecutableGraphQL
                }
                _ => file_kind,
            };

            tracing::info!("Adding to host: workspace={}, uri={:?}, content_len={}, file_kind={:?}, line_offset={}",
                workspace_uri, uri, final_content.len(), final_kind, line_offset);
            self.add_file_to_host(
                &workspace_uri,
                &uri,
                &final_content,
                final_kind,
                line_offset,
            )
            .await;
        }

        self.validate_document(uri).await;
    }

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let start = std::time::Instant::now();
        tracing::info!("Document changed");

        // Get the latest content from changes (full sync mode)
        for change in params.content_changes {
            tracing::info!(
                change_len = change.text.len(),
                first_100 = ?&change.text[..100.min(change.text.len())],
                "Processing content change"
            );

            // Update AnalysisHost
            if let Some((workspace_uri, _project_idx)) = self.find_workspace_and_project(&uri) {
                let _file_path = uri.to_file_path();

                // Get extract config from host
                let extract_config = if let Some(host_mutex) = self.hosts.get(&workspace_uri) {
                    let host = host_mutex.lock().await;
                    host.get_extract_config()
                } else {
                    graphql_extract::ExtractConfig::default()
                };

                // Determine file kind by inspecting path and content
                let file_kind =
                    Self::determine_file_kind_from_content(uri.path().as_str(), &change.text);
                tracing::info!("did_change determined file_kind: {:?}", file_kind);

                // Extract GraphQL from TypeScript/JavaScript files
                tracing::info!(
                    "did_change about to extract GraphQL, path={}",
                    uri.path().as_str()
                );
                let (final_content, line_offset) = Self::extract_graphql_from_source(
                    uri.path().as_str(),
                    &change.text,
                    &extract_config,
                );
                tracing::info!(
                    extracted_len = final_content.len(),
                    line_offset = line_offset,
                    first_100 = ?&final_content[..100.min(final_content.len())],
                    "did_change extracted GraphQL content"
                );

                // IMPORTANT: After extraction, the content is pure GraphQL, so change the kind to ExecutableGraphQL
                let final_kind = match file_kind {
                    graphql_ide::FileKind::TypeScript | graphql_ide::FileKind::JavaScript
                        if !final_content.is_empty() =>
                    {
                        graphql_ide::FileKind::ExecutableGraphQL
                    }
                    _ => file_kind,
                };

                self.add_file_to_host(
                    &workspace_uri,
                    &uri,
                    &final_content,
                    final_kind,
                    line_offset,
                )
                .await;
            }

            // Validate immediately - Salsa's incremental computation makes this fast
            self.validate_document(uri.clone()).await;
        }

        tracing::debug!(
            elapsed_ms = start.elapsed().as_millis(),
            "Completed did_change with validation"
        );
    }

    #[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        tracing::info!("Document saved: {:?}", params.text_document.uri);
        // NOTE: Cross-file dependency tracking and revalidation should be handled
        // by the Analysis layer, not the LSP layer. For now, files are only revalidated
        // when they are opened or changed.
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        tracing::info!("Document closed: {:?}", params.text_document.uri);

        // NOTE: We intentionally do NOT remove the file from AnalysisHost when it's closed.
        // The file is still part of the project on disk, and other files may reference
        // fragments/types defined in it. Only files that are deleted from disk should be
        // removed from the analysis.

        // Clear diagnostics for the closed file
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
                        tracing::info!("Config file changed for workspace: {}", workspace_uri);
                        // TODO: Reload config and re-validate all documents
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

        // Find workspace for this document
        let Some((workspace_uri, _)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get AnalysisHost and create snapshot (new architecture)
        let host = self.get_or_create_host(&workspace_uri);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
        };

        // Convert LSP position to graphql-ide position
        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        // Get completions from Analysis
        let Some(items) = analysis.completions(&file_path, position) else {
            return Ok(None);
        };

        // Convert graphql-ide completion items to LSP completion items
        let lsp_items: Vec<lsp_types::CompletionItem> =
            items.into_iter().map(convert_ide_completion_item).collect();

        Ok(Some(CompletionResponse::Array(lsp_items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let lsp_position = params.text_document_position_params.position;

        tracing::debug!("Hover requested: {:?} at {:?}", uri, lsp_position);

        // Find workspace for this document
        let Some((workspace_uri, _)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get AnalysisHost and create snapshot (new architecture)
        let host = self.get_or_create_host(&workspace_uri);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
        };

        // Convert LSP position to graphql-ide position
        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        // Get hover from Analysis
        let Some(hover_result) = analysis.hover(&file_path, position) else {
            return Ok(None);
        };

        // Convert graphql-ide HoverResult to LSP Hover
        let hover = convert_ide_hover(hover_result);

        Ok(Some(hover))
    }

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

        // Find workspace for this document
        let Some((workspace_uri, _)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get AnalysisHost and create snapshot (new architecture)
        let host = self.get_or_create_host(&workspace_uri);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
        };

        // Convert LSP position to graphql-ide position
        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        // Get goto definition from Analysis
        let Some(locations) = analysis.goto_definition(&file_path, position) else {
            tracing::info!(
                "Goto definition completed in {:?}, returning 0 locations",
                start.elapsed()
            );
            return Ok(None);
        };

        // Convert graphql-ide Locations to LSP Locations
        let lsp_locations: Vec<Location> = locations
            .iter()
            .map(|loc| {
                tracing::info!(
                    "Returning location: file={}, range={}:{} to {}:{}",
                    loc.file.as_str(),
                    loc.range.start.line,
                    loc.range.start.character,
                    loc.range.end.line,
                    loc.range.end.character
                );
                convert_ide_location(loc)
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

        // Find workspace for this document
        let Some((workspace_uri, _)) = self.find_workspace_and_project(&uri) else {
            tracing::warn!("No project found for document: {:?}", uri);
            return Ok(None);
        };

        // Get AnalysisHost and create snapshot (new architecture)
        let host = self.get_or_create_host(&workspace_uri);
        let analysis = {
            let host_guard = host.lock().await;
            host_guard.snapshot()
        };

        // Convert LSP position to graphql-ide position
        let position = convert_lsp_position(lsp_position);
        let file_path = graphql_ide::FilePath::new(uri.to_string());

        // Find references from Analysis
        let Some(locations) = analysis.find_references(&file_path, position, include_declaration)
        else {
            tracing::info!("No references found at position {:?}", position);
            return Ok(None);
        };

        // Convert graphql-ide Locations to LSP Locations
        let lsp_locations: Vec<Location> = locations
            .into_iter()
            .map(|loc| convert_ide_location(&loc))
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

                // Project information removed (old system)
            }

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

#[cfg(test)]
mod tests {
    use super::GraphQLLanguageServer;

    #[test]
    fn test_extract_fragment_names_from_content() {
        // Test with multiple fragments
        let content = r"
            fragment UserBasic on User {
                id
                name
            }

            fragment UserDetailed on User {
                ...UserBasic
                email
                posts {
                    ...PostBasic
                }
            }

            query GetUser {
                user {
                    ...UserDetailed
                }
            }
        ";

        let fragments = GraphQLLanguageServer::extract_fragment_names_from_content(content);
        assert_eq!(fragments.len(), 2);
        assert!(fragments.contains("UserBasic"));
        assert!(fragments.contains("UserDetailed"));
        assert!(!fragments.contains("PostBasic")); // Referenced but not defined

        // Test with no fragments
        let content_no_fragments = r"
            query GetUser {
                user {
                    id
                    name
                }
            }
        ";

        let fragments =
            GraphQLLanguageServer::extract_fragment_names_from_content(content_no_fragments);
        assert_eq!(fragments.len(), 0);
    }

    #[test]
    fn test_document_references_fragments() {
        // Document that references BattleDetailed
        let content_with_reference = r"
            mutation StartBattle($trainer1Id: ID!, $trainer2Id: ID!) {
                startBattle(trainer1Id: $trainer1Id, trainer2Id: $trainer2Id) {
                    ...BattleDetailed
                }
            }
        ";

        let mut changed_fragments = std::collections::HashSet::new();
        changed_fragments.insert("BattleDetailed".to_string());

        assert!(
            GraphQLLanguageServer::document_references_fragments(
                content_with_reference,
                &changed_fragments
            ),
            "Document should reference BattleDetailed fragment"
        );

        // Document that doesn't reference BattleDetailed
        let content_without_reference = r"
            query GetPokemon($id: ID!) {
                pokemon(id: $id) {
                    id
                    name
                }
            }
        ";

        assert!(
            !GraphQLLanguageServer::document_references_fragments(
                content_without_reference,
                &changed_fragments
            ),
            "Document should not reference BattleDetailed fragment"
        );

        // Document with nested fragment spreads
        let content_nested = r"
            fragment TrainerWithBattles on Trainer {
                id
                name
                battles {
                    ...BattleDetailed
                }
            }
        ";

        assert!(
            GraphQLLanguageServer::document_references_fragments(
                content_nested,
                &changed_fragments
            ),
            "Fragment definition should reference BattleDetailed fragment"
        );

        // Multiple fragments, only one matches
        let mut multiple_fragments = std::collections::HashSet::new();
        multiple_fragments.insert("BattleDetailed".to_string());
        multiple_fragments.insert("PokemonInfo".to_string());

        assert!(
            GraphQLLanguageServer::document_references_fragments(
                content_with_reference,
                &multiple_fragments
            ),
            "Document should reference at least one of the changed fragments"
        );
    }

    #[test]
    fn test_document_references_fragments_no_match() {
        let content = r"
            query GetPokemon {
                pokemon {
                    id
                    ...PokemonBasic
                }
            }
        ";

        let mut changed_fragments = std::collections::HashSet::new();
        changed_fragments.insert("BattleDetailed".to_string());

        assert!(
            !GraphQLLanguageServer::document_references_fragments(content, &changed_fragments),
            "Document uses PokemonBasic but not BattleDetailed"
        );
    }
}
