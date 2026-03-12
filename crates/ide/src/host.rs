use std::sync::Arc;

use parking_lot::RwLock;
use salsa::Setter;

use graphql_base_db::{DocumentKind, Language};

use crate::analysis::Analysis;
use crate::database::{ExtractConfigInput, IdeDatabase, LintConfigInput};
use crate::discovery::{
    determine_document_file_kind, expand_braces, path_to_file_path, DiscoveredFile, LoadedFile,
};
use crate::file_registry::FileRegistry;
use crate::helpers::path_to_file_uri;
use crate::types::{
    DocumentLoadResult, FilePath, PendingIntrospection, SchemaContentError, SchemaLoadResult,
};

/// The main analysis host
///
/// This is the entry point for all IDE features. It owns the database and
/// provides methods to apply changes and create snapshots for analysis.
///
/// # Snapshot Lifecycle (Important!)
///
/// This follows Salsa's single-writer, multi-reader model:
/// - Call [`snapshot()`](Self::snapshot) to create an immutable [`Analysis`] for queries
/// - **All snapshots must be dropped before calling any mutating method**
/// - Failing to drop snapshots before mutation will cause a hang/deadlock
///
/// ```ignore
/// // CORRECT: Scope snapshots so they're dropped before mutation
/// let result = {
///     let snapshot = host.snapshot();
///     snapshot.diagnostics(&file)
/// }; // snapshot dropped here
/// host.add_file(&file, new_content, kind); // Safe: no snapshots exist
///
/// // WRONG: Holding snapshot across mutation
/// let snapshot = host.snapshot();
/// let result = snapshot.diagnostics(&file);
/// host.add_file(&file, new_content, kind); // HANGS: snapshot still alive!
/// ```
pub struct AnalysisHost {
    db: IdeDatabase,
    /// File registry for mapping paths to file IDs
    /// Wrapped in Arc<RwLock> so snapshots can share it
    registry: Arc<RwLock<FileRegistry>>,
}

impl AnalysisHost {
    /// Create a new analysis host with a default database
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: IdeDatabase::default(),
            registry: Arc::new(RwLock::new(FileRegistry::new())),
        }
    }

    /// Add or update a file in the host
    ///
    /// This is a convenience method for adding files to the registry and database.
    ///
    /// Returns `true` if this is a new file, `false` if it's an update to an existing file.
    ///
    /// **IMPORTANT**: Only call `rebuild_project_files()` when this returns `true` (new file).
    /// Content-only updates do NOT require rebuilding the project index.
    pub fn add_file(
        &mut self,
        path: &FilePath,
        content: &str,
        language: Language,
        document_kind: DocumentKind,
    ) -> bool {
        let mut registry = self.registry.write();
        let (_, _, _, is_new) =
            registry.add_file(&mut self.db, path, content, language, document_kind);
        is_new
    }

    /// Batch-add pre-discovered files to the host.
    ///
    /// This is more efficient than calling `add_file` in a loop because
    /// file I/O has already been done. The lock is only held briefly
    /// for registration, not during disk reads.
    ///
    /// Returns the list of `LoadedFile` structs for building indexes.
    pub fn add_discovered_files(&mut self, files: &[DiscoveredFile]) -> Vec<LoadedFile> {
        let mut registry = self.registry.write();
        let mut loaded = Vec::with_capacity(files.len());

        for file in files {
            registry.add_file(
                &mut self.db,
                &file.path,
                &file.content,
                file.language,
                file.document_kind,
            );
            loaded.push(LoadedFile {
                path: file.path.clone(),
                language: file.language,
                document_kind: file.document_kind,
            });
        }

        loaded
    }

    /// Rebuild the `ProjectFiles` index after adding/removing files
    ///
    /// This should be called after batch adding files to avoid O(n^2) performance.
    /// It's relatively expensive as it iterates through all files, so avoid calling
    /// it in a loop.
    ///
    /// This method also syncs the `ProjectFiles` to the database so queries can
    /// access it via `db.project_files()`.
    pub fn rebuild_project_files(&mut self) {
        let mut registry = self.registry.write();
        registry.rebuild_project_files(&mut self.db);

        // Sync project_files from registry to database
        // This enables queries to access project_files via db.project_files()
        self.db.project_files_input = registry.project_files();
    }

    /// Add multiple files in batch, then rebuild the project index once
    ///
    /// This is the recommended way to load multiple files at once. It:
    /// 1. Adds all files to the registry without rebuilding
    /// 2. Rebuilds the project index once at the end
    ///
    /// This is O(n) instead of O(n^2) compared to calling `add_file` + `rebuild_project_files`
    /// for each file individually.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use graphql_ide::{AnalysisHost, FilePath, Language, DocumentKind};
    ///
    /// let mut host = AnalysisHost::new();
    /// let files = vec![
    ///     (FilePath::new("file:///schema.graphql"), "type Query { hello: String }", Language::GraphQL, DocumentKind::Schema),
    ///     (FilePath::new("file:///query.graphql"), "query { hello }", Language::GraphQL, DocumentKind::Executable),
    /// ];
    /// host.add_files_batch(&files);
    /// ```
    pub fn add_files_batch(&mut self, files: &[(FilePath, &str, Language, DocumentKind)]) {
        let mut registry = self.registry.write();
        let mut any_new = false;

        for (path, content, language, document_kind) in files {
            let (_, _, _, is_new) =
                registry.add_file(&mut self.db, path, content, *language, *document_kind);
            any_new = any_new || is_new;
        }

        // Only rebuild if at least one file was new
        if any_new {
            registry.rebuild_project_files(&mut self.db);
            // Sync project_files from registry to database
            self.db.project_files_input = registry.project_files();
        }
    }

    /// Update a file and optionally create a snapshot in a single lock acquisition
    ///
    /// This is optimized for the common case of editing an existing file. It:
    /// 1. Updates the file content (cheap operation)
    /// 2. Returns a snapshot for immediate analysis
    ///
    /// This avoids the overhead of multiple lock acquisitions per keystroke.
    /// For new files, call `add_file()` followed by `rebuild_project_files()` instead.
    ///
    /// Returns `(is_new_file, Analysis)` tuple.
    pub fn update_file_and_snapshot(
        &mut self,
        path: &FilePath,
        content: &str,
        language: Language,
        document_kind: DocumentKind,
    ) -> (bool, Analysis) {
        // Single lock acquisition for both operations
        let mut registry = self.registry.write();
        let (_, _, _, is_new) =
            registry.add_file(&mut self.db, path, content, language, document_kind);

        // If this is a new file, rebuild the index before creating snapshot
        // This also syncs project_files to self.db.project_files_input
        if is_new {
            registry.rebuild_project_files(&mut self.db);
            // Sync project_files from registry to database
            self.db.project_files_input = registry.project_files();
        }

        let project_files = self.db.project_files_input;
        // Release the lock before creating the snapshot (no longer needed)
        drop(registry);

        let snapshot = Analysis {
            db: self.db.clone(),
            registry: Arc::clone(&self.registry),
            project_files,
        };

        (is_new, snapshot)
    }

    /// Check if a file exists in this host's registry
    #[must_use]
    pub fn contains_file(&self, path: &FilePath) -> bool {
        let registry = self.registry.read();
        registry.get_file_id(path).is_some()
    }

    /// Remove a file from the host
    pub fn remove_file(&mut self, path: &FilePath) {
        let mut registry = self.registry.write();
        if let Some(file_id) = registry.get_file_id(path) {
            registry.remove_file(file_id);
            registry.rebuild_project_files(&mut self.db);
        }
    }

    /// Load schema files from a project configuration
    ///
    /// This method:
    /// - Always includes Apollo Client built-in directives
    /// - Loads schema files from local paths (single file, multiple files, glob patterns)
    /// - Supports TypeScript/JavaScript files with embedded GraphQL schemas
    /// - Collects remote introspection configs for async fetching by the caller
    ///
    /// Returns a [`SchemaLoadResult`] containing the count of loaded files and any
    /// pending introspection configurations that require async fetching.
    pub fn load_schemas_from_config(
        &mut self,
        config: &graphql_config::ProjectConfig,
        base_dir: &std::path::Path,
    ) -> anyhow::Result<SchemaLoadResult> {
        const SCHEMA_BUILTINS: &str = include_str!("schema_builtins.graphql");
        const APOLLO_CLIENT_BUILTINS: &str = include_str!("apollo_client_builtins.graphql");
        const RELAY_CLIENT_BUILTINS: &str = include_str!("relay_client_builtins.graphql");

        // Always include GraphQL spec built-in directives first (e.g., @oneOf)
        self.add_file(
            &FilePath::new("schema_builtins.graphql".to_string()),
            SCHEMA_BUILTINS,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let mut count = 1;

        // Include client-specific built-in directives based on config
        match config.client() {
            Some(graphql_config::ClientConfig::Apollo) => {
                self.add_file(
                    &FilePath::new("client_builtins.graphql".to_string()),
                    APOLLO_CLIENT_BUILTINS,
                    Language::GraphQL,
                    DocumentKind::Schema,
                );
                count += 1;
            }
            Some(graphql_config::ClientConfig::Relay) => {
                self.add_file(
                    &FilePath::new("client_builtins.graphql".to_string()),
                    RELAY_CLIENT_BUILTINS,
                    Language::GraphQL,
                    DocumentKind::Schema,
                );
                count += 1;
            }
            Some(graphql_config::ClientConfig::None) | None => {
                // No client directives
            }
        }
        let mut loaded_paths = Vec::new();
        let mut pending_introspections = Vec::new();
        let mut content_errors = Vec::new();
        let mut unmatched_patterns = Vec::new();

        let patterns: Vec<String> = match &config.schema {
            graphql_config::SchemaConfig::Path(s) => vec![s.clone()],
            graphql_config::SchemaConfig::Paths(arr) => arr.clone(),
            graphql_config::SchemaConfig::Introspection(introspection) => {
                // Collect introspection config for async fetching by the caller
                tracing::info!(
                    "Found remote schema introspection config: {}",
                    introspection.url
                );
                pending_introspections.push(PendingIntrospection::from_config(introspection));
                vec![]
            }
        };

        for pattern in patterns {
            // Collect URL patterns as pending introspections for async fetching
            if pattern.starts_with("http://") || pattern.starts_with("https://") {
                tracing::info!("Found remote schema URL: {}", pattern);
                pending_introspections.push(PendingIntrospection {
                    url: pattern,
                    headers: None,
                    timeout: None,
                    retry: None,
                });
                continue;
            }

            // Treat as file glob pattern
            let full_pattern = base_dir.join(&pattern).display().to_string();

            match glob::glob(&full_pattern) {
                Ok(paths) => {
                    let mut pattern_matched_files = false;
                    for entry in paths.flatten() {
                        if entry.is_file() {
                            pattern_matched_files = true;
                            match std::fs::read_to_string(&entry) {
                                Ok(content) => {
                                    let file_uri = path_to_file_uri(&entry);
                                    let language = graphql_extract::Language::from_path(&entry);

                                    // Check if this is a TS/JS file that needs extraction
                                    if let Some(lang) = language {
                                        if lang.requires_extraction() {
                                            // Extract GraphQL from TS/JS file
                                            let extract_config = self.get_extract_config();
                                            match graphql_extract::extract_from_source(
                                                &content,
                                                lang,
                                                &extract_config,
                                                &file_uri,
                                            ) {
                                                Ok(blocks) => {
                                                    // Validate all blocks for executable definitions
                                                    let all_sources: String = blocks
                                                        .iter()
                                                        .map(|b| b.source.as_str())
                                                        .collect::<Vec<_>>()
                                                        .join("\n");
                                                    if let Some(mismatch) = graphql_syntax::validate_content_matches_kind(
                                                        &all_sources,
                                                        DocumentKind::Schema,
                                                    ) {
                                                        let definitions = match mismatch {
                                                            graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable { definitions } => definitions,
                                                            graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema { .. } => Vec::new(),
                                                        };
                                                        content_errors.push(SchemaContentError {
                                                            pattern: pattern.clone(),
                                                            file_path: entry.clone(),
                                                            unexpected_definitions: definitions,
                                                        });
                                                    }

                                                    if blocks.len() == 1 {
                                                        // Single block: store original TS/JS content
                                                        // so the syntax crate can handle extraction
                                                        // with proper line offsets
                                                        self.add_file(
                                                            &FilePath::new(file_uri.clone()),
                                                            &content,
                                                            lang,
                                                            DocumentKind::Schema,
                                                        );
                                                        count += 1;
                                                    } else {
                                                        // Multiple blocks: create separate entries
                                                        // with line range URIs for each block
                                                        for block in &blocks {
                                                            let start_line =
                                                                block.location.range.start.line + 1;
                                                            let end_line =
                                                                block.location.range.end.line + 1;
                                                            let block_uri = format!(
                                                                "{file_uri}#L{start_line}-L{end_line}"
                                                            );

                                                            self.add_file(
                                                                &FilePath::new(block_uri),
                                                                &block.source,
                                                                Language::GraphQL,
                                                                DocumentKind::Schema,
                                                            );
                                                            count += 1;
                                                        }
                                                    }
                                                    if blocks.is_empty() {
                                                        tracing::debug!(
                                                            "No GraphQL blocks found in {}",
                                                            entry.display()
                                                        );
                                                    } else {
                                                        loaded_paths.push(entry.clone());
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        "Failed to extract GraphQL from {}: {}",
                                                        entry.display(),
                                                        e
                                                    );
                                                }
                                            }
                                            continue;
                                        }
                                    }

                                    // Pure GraphQL file - validate and add
                                    // Check for executable definitions (operations/fragments)
                                    if let Some(mismatch) =
                                        graphql_syntax::validate_content_matches_kind(
                                            &content,
                                            DocumentKind::Schema,
                                        )
                                    {
                                        let definitions = match mismatch {
                                            graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable { definitions } => definitions,
                                            graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema { .. } => Vec::new(),
                                        };
                                        content_errors.push(SchemaContentError {
                                            pattern: pattern.clone(),
                                            file_path: entry.clone(),
                                            unexpected_definitions: definitions,
                                        });
                                    }

                                    self.add_file(
                                        &FilePath::new(file_uri),
                                        &content,
                                        Language::GraphQL,
                                        DocumentKind::Schema,
                                    );
                                    loaded_paths.push(entry.clone());
                                    count += 1;
                                }
                                Err(e) => {
                                    let path_display = entry.display().to_string();
                                    tracing::error!(
                                        "Failed to read schema file {path_display}: {e}"
                                    );
                                    return Err(anyhow::anyhow!(
                                        "Failed to read schema file {path_display}: {e}"
                                    ));
                                }
                            }
                        }
                    }
                    if !pattern_matched_files {
                        tracing::debug!(
                            "Schema pattern matched no files: {} (expanded: {})",
                            pattern,
                            full_pattern
                        );
                        unmatched_patterns.push(pattern.clone());
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to expand glob pattern {full_pattern}: {e}");
                    return Err(anyhow::anyhow!(
                        "Failed to expand glob pattern {full_pattern}: {e}"
                    ));
                }
            }
        }

        tracing::info!(
            "Loaded {} schema file(s) ({} paths tracked), {} pending introspection(s)",
            count,
            loaded_paths.len(),
            pending_introspections.len()
        );
        Ok(SchemaLoadResult {
            loaded_count: count,
            loaded_paths,
            pending_introspections,
            content_errors,
            unmatched_patterns,
        })
    }

    /// Add an introspected schema as a virtual file.
    ///
    /// This method registers a schema fetched via introspection using a virtual URI
    /// in the format `schema://<host>/<path>/schema.graphql`. The `.graphql`
    /// extension ensures editors recognize the file for syntax highlighting and
    /// language features.
    ///
    /// # Arguments
    ///
    /// * `url` - The original GraphQL endpoint URL (used to generate the virtual URI)
    /// * `sdl` - The schema SDL obtained from introspection
    ///
    /// # Returns
    ///
    /// The virtual file URI used to register the schema.
    pub fn add_introspected_schema(&mut self, url: &str, sdl: &str) -> String {
        let virtual_uri = format!(
            "schema://{}/schema.graphql",
            url.trim_start_matches("https://")
                .trim_start_matches("http://")
        );
        tracing::info!("Adding introspected schema from {} as {}", url, virtual_uri);
        self.add_file(
            &FilePath::new(virtual_uri.clone()),
            sdl,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        virtual_uri
    }

    /// Set the lint configuration for the project
    ///
    /// This properly invalidates all queries that depend on lint config via Salsa's
    /// dependency tracking. Only lint-dependent queries will re-run when config changes.
    pub fn set_lint_config(&mut self, config: graphql_linter::LintConfig) {
        if let Some(input) = self.db.lint_config_input {
            input.set_config(&mut self.db).to(Arc::new(config));
        } else {
            let input = LintConfigInput::new(&self.db, Arc::new(config));
            self.db.lint_config_input = Some(input);
        }
    }

    /// Set the extract configuration for the project
    ///
    /// This properly invalidates all queries that depend on extract config via Salsa's
    /// dependency tracking. Only extract-dependent queries will re-run when config changes.
    pub fn set_extract_config(&mut self, config: graphql_extract::ExtractConfig) {
        if let Some(input) = self.db.extract_config_input {
            input.set_config(&mut self.db).to(Arc::new(config));
        } else {
            let input = ExtractConfigInput::new(&self.db, Arc::new(config));
            self.db.extract_config_input = Some(input);
        }
    }

    /// Get the extract configuration for the project
    pub fn get_extract_config(&self) -> graphql_extract::ExtractConfig {
        self.db
            .extract_config_input
            .map(|input| (*input.config(&self.db)).clone())
            .unwrap_or_default()
    }

    /// Load document files from a project configuration
    ///
    /// This method handles:
    /// - Glob pattern expansion (including brace expansion like `{ts,tsx}`)
    /// - File reading and content extraction
    /// - File kind determination based on extension
    /// - Batch file registration (more efficient than individual `add_file` calls)
    ///
    /// Returns information about loaded files for indexing purposes.
    ///
    /// # Arguments
    ///
    /// * `config` - The project configuration containing document patterns
    /// * `workspace_path` - The base directory for glob pattern resolution
    ///
    /// # Returns
    ///
    /// A vector of `LoadedFile` structs containing file paths and metadata.
    /// The caller can use this information to build file-to-project indexes.
    pub fn load_documents_from_config(
        &mut self,
        config: &graphql_config::ProjectConfig,
        workspace_path: &std::path::Path,
    ) -> (Vec<LoadedFile>, DocumentLoadResult) {
        let Some(documents_config) = &config.documents else {
            return (Vec::new(), DocumentLoadResult::default());
        };

        let patterns: Vec<String> = documents_config
            .patterns()
            .into_iter()
            .map(std::string::ToString::to_string)
            .collect();

        let mut loaded_files: Vec<LoadedFile> = Vec::new();
        let mut files_to_add: Vec<(FilePath, String, Language, DocumentKind)> = Vec::new();
        let mut unmatched_patterns: Vec<String> = Vec::new();

        for pattern in patterns {
            // Skip negation patterns
            if pattern.trim().starts_with('!') {
                continue;
            }

            let expanded_patterns = expand_braces(&pattern);
            let mut pattern_matched_any_files = false;

            for expanded_pattern in expanded_patterns {
                let full_pattern = workspace_path.join(&expanded_pattern);

                match glob::glob(&full_pattern.display().to_string()) {
                    Ok(paths) => {
                        for entry in paths {
                            match entry {
                                Ok(path) if path.is_file() => {
                                    // Skip node_modules
                                    if path.components().any(|c| c.as_os_str() == "node_modules") {
                                        continue;
                                    }

                                    // Read file content
                                    match std::fs::read_to_string(&path) {
                                        Ok(content) => {
                                            let path_str = path.display().to_string();
                                            let (language, document_kind) =
                                                determine_document_file_kind(&path_str, &content);

                                            let file_path = path_to_file_path(&path);

                                            pattern_matched_any_files = true;
                                            loaded_files.push(LoadedFile {
                                                path: file_path.clone(),
                                                language,
                                                document_kind,
                                            });

                                            files_to_add.push((
                                                file_path,
                                                content,
                                                language,
                                                document_kind,
                                            ));
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
                        tracing::error!("Invalid glob pattern '{}': {}", expanded_pattern, e);
                    }
                }
            }

            if !pattern_matched_any_files {
                tracing::debug!("Document pattern matched no files: {}", pattern);
                unmatched_patterns.push(pattern.clone());
            }
        }

        // Batch add all files using add_files_batch for O(n) performance
        // Convert owned strings to borrowed for the batch API
        let batch_refs: Vec<(FilePath, &str, Language, DocumentKind)> = files_to_add
            .iter()
            .map(|(path, content, language, document_kind)| {
                (path.clone(), content.as_str(), *language, *document_kind)
            })
            .collect();
        self.add_files_batch(&batch_refs);

        let result = DocumentLoadResult {
            loaded_count: loaded_files.len(),
            unmatched_patterns,
        };

        (loaded_files, result)
    }

    /// Iterate over all files in the host
    ///
    /// Returns an iterator of `FilePath` for all registered files.
    pub fn files(&self) -> Vec<FilePath> {
        let registry = self.registry.read();
        registry
            .all_file_ids()
            .into_iter()
            .filter_map(|file_id| registry.get_path(file_id))
            .collect()
    }

    /// Get an immutable snapshot for analysis
    ///
    /// This snapshot can be used from multiple threads and provides all IDE features.
    /// It's cheap to create and clone (`RootDatabase` implements Clone via salsa).
    ///
    /// # Lifecycle Warning
    ///
    /// The returned `Analysis` **must be dropped before calling any mutating method**
    /// on this `AnalysisHost`. This is required by Salsa's single-writer model.
    /// See the struct-level documentation for details and examples.
    pub fn snapshot(&self) -> Analysis {
        // project_files is already synced to the database in rebuild_project_files()
        // Queries access it via db.project_files()
        let project_files = self.db.project_files_input;

        if let Some(ref pf) = project_files {
            let doc_count = pf.document_file_ids(&self.db).ids(&self.db).len();
            let schema_count = pf.schema_file_ids(&self.db).ids(&self.db).len();
            tracing::debug!(
                "Snapshot project_files: {} schema files, {} document files",
                schema_count,
                doc_count
            );
        } else {
            tracing::warn!("Snapshot project_files is None!");
        }

        Analysis {
            db: self.db.clone(),
            registry: Arc::clone(&self.registry),
            project_files,
        }
    }
}

impl Default for AnalysisHost {
    fn default() -> Self {
        Self::new()
    }
}
