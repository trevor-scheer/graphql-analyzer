use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use salsa::Setter;

use graphql_base_db::{DocumentKind, Language};

/// Global counter for snapshot IDs to track creation and drop in logs.
static SNAPSHOT_ID: AtomicU64 = AtomicU64::new(1);

fn next_snapshot_id() -> u64 {
    SNAPSHOT_ID.fetch_add(1, Ordering::Relaxed)
}

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
    /// File registry for mapping paths to file IDs.
    ///
    /// Owned directly (no inner lock) because the outer `tokio::sync::Mutex`
    /// in `ProjectHost` already serializes mutators, and snapshots no longer
    /// reach back into the registry — they read everything via Salsa inputs
    /// (`FilePathMap`, `FileEntryMap`).
    registry: FileRegistry,
}

impl AnalysisHost {
    /// Create a new analysis host with a default database
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: IdeDatabase::default(),
            registry: FileRegistry::new(),
        }
    }

    /// Add or update a file in the host.
    ///
    /// Returns `true` if this is a new file, `false` if it's an update to an
    /// existing file. New files automatically trigger a `ProjectFiles` rebuild
    /// so that subsequent `snapshot()` calls observe the new file.
    pub fn add_file(
        &mut self,
        path: &FilePath,
        content: &str,
        language: Language,
        document_kind: DocumentKind,
    ) -> bool {
        let (_, _, _, is_new) =
            self.registry
                .add_file(&mut self.db, path, content, language, document_kind);
        if is_new {
            self.sync_project_files();
        }
        is_new
    }

    /// Batch-add pre-discovered files to the host.
    ///
    /// More efficient than calling `add_file` in a loop because the project
    /// index is rebuilt once at the end instead of after every file.
    pub fn add_discovered_files(&mut self, files: &[DiscoveredFile]) -> Vec<LoadedFile> {
        let mut loaded = Vec::with_capacity(files.len());
        let mut any_new = false;

        for file in files {
            let (_, _, _, is_new) = self.registry.add_file(
                &mut self.db,
                &file.path,
                &file.content,
                file.language,
                file.document_kind,
            );
            any_new = any_new || is_new;
            loaded.push(LoadedFile {
                path: file.path.clone(),
                language: file.language,
                document_kind: file.document_kind,
            });
        }

        if any_new {
            self.sync_project_files();
        }
        loaded
    }

    /// Rebuild the `ProjectFiles` Salsa input from the current registry state.
    ///
    /// Most callers do NOT need to invoke this directly — `add_file`,
    /// `add_files_batch`, `add_discovered_files`, `remove_file`, and
    /// `update_file_and_snapshot` all maintain the index automatically. It is
    /// kept on the public API for backwards compatibility and for the rare
    /// "I mutated the registry through some other path, please refresh"
    /// scenario.
    pub fn rebuild_project_files(&mut self) {
        self.sync_project_files();
    }

    /// Internal: rebuild the `ProjectFiles` index and sync the cached input
    /// reference on the database.
    fn sync_project_files(&mut self) {
        self.registry.rebuild_project_files(&mut self.db);
        self.db.project_files_input = self.registry.project_files();
    }

    /// Add multiple files in batch, then rebuild the project index once.
    ///
    /// This is O(n) instead of O(n^2) compared to calling `add_file` repeatedly,
    /// because the `ProjectFiles` Salsa input is rebuilt only once at the end.
    pub fn add_files_batch(&mut self, files: &[(FilePath, &str, Language, DocumentKind)]) {
        let mut any_new = false;
        for (path, content, language, document_kind) in files {
            let (_, _, _, is_new) =
                self.registry
                    .add_file(&mut self.db, path, content, *language, *document_kind);
            any_new = any_new || is_new;
        }
        if any_new {
            self.sync_project_files();
        }
    }

    /// Update a file and create a snapshot in one shot.
    ///
    /// Optimized for `did_change`: if the file already exists, this only bumps
    /// the file's `FileContent` (no project index rebuild). For a new file
    /// (`did_open`) it rebuilds the index before snapshotting so the snapshot
    /// observes the new file.
    ///
    /// Returns `(is_new_file, Analysis)`.
    pub fn update_file_and_snapshot(
        &mut self,
        path: &FilePath,
        content: &str,
        language: Language,
        document_kind: DocumentKind,
    ) -> (bool, Analysis) {
        let (_, _, _, is_new) =
            self.registry
                .add_file(&mut self.db, path, content, language, document_kind);
        if is_new {
            self.sync_project_files();
        }
        (is_new, self.snapshot())
    }

    /// Check if a file exists in this host's registry
    #[must_use]
    pub fn contains_file(&self, path: &FilePath) -> bool {
        self.registry.get_file_id(path).is_some()
    }

    /// Remove a file from the host
    pub fn remove_file(&mut self, path: &FilePath) {
        if let Some(file_id) = self.registry.get_file_id(path) {
            self.registry.remove_file(file_id);
            self.sync_project_files();
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

                                    // JSON introspection result file support
                                    if entry.extension().and_then(|e| e.to_str()) == Some("json")
                                        && graphql_introspect::is_introspection_json(&content)
                                    {
                                        match graphql_introspect::introspection_json_to_sdl(
                                            &content,
                                        ) {
                                            Ok(sdl) => {
                                                tracing::info!(
                                                    "Loaded JSON introspection result from {}",
                                                    entry.display()
                                                );
                                                self.add_file(
                                                    &FilePath::new(file_uri),
                                                    &sdl,
                                                    Language::GraphQL,
                                                    DocumentKind::Schema,
                                                );
                                                loaded_paths.push(entry.clone());
                                                count += 1;
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Failed to parse JSON introspection result from {}: {}",
                                                    entry.display(),
                                                    e
                                                );
                                            }
                                        }
                                        continue;
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

        // Load resolved schema file if configured
        if let Some(resolved_path) = config.resolved_schema() {
            let resolved_full = base_dir.join(&resolved_path);
            if resolved_full.is_file() {
                match std::fs::read_to_string(&resolved_full) {
                    Ok(resolved_content) => {
                        let file_uri = path_to_file_uri(&resolved_full);
                        let file_path = FilePath::new(file_uri);
                        let (file_id, _, _, _) = self.registry.add_file(
                            &mut self.db,
                            &file_path,
                            &resolved_content,
                            Language::GraphQL,
                            DocumentKind::Schema,
                        );
                        self.registry.mark_as_resolved_schema(file_id);
                        loaded_paths.push(resolved_full);
                        count += 1;
                        tracing::info!("Loaded resolved schema from '{}'", resolved_path);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read resolved schema '{}': {}",
                            resolved_full.display(),
                            e
                        );
                    }
                }
            } else {
                tracing::debug!(
                    "Resolved schema file not found (yet): {}",
                    resolved_full.display()
                );
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
        extract_config: &graphql_extract::ExtractConfig,
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

                                            // Skip files that require extraction but contain no GraphQL
                                            if language.requires_extraction() {
                                                let blocks = graphql_extract::extract_from_source(
                                                    &content,
                                                    language,
                                                    extract_config,
                                                    &path_str,
                                                )
                                                .unwrap_or_default();
                                                if blocks.is_empty() {
                                                    continue;
                                                }
                                            }

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

    /// Iterate over all files in the host.
    pub fn files(&self) -> Vec<FilePath> {
        self.registry
            .all_file_ids()
            .into_iter()
            .filter_map(|file_id| self.registry.get_path(file_id))
            .collect()
    }

    /// Get an immutable snapshot for analysis.
    ///
    /// Cheap: creates a Salsa db clone and copies the cached `ProjectFiles`
    /// handle. The snapshot resolves all file lookups through Salsa, never
    /// reaching back into the host's `FileRegistry`.
    ///
    /// # Lifecycle
    ///
    /// The returned `Analysis` must be dropped before any host mutation. This
    /// is enforced by Salsa's single-writer model: setters block until all
    /// outstanding snapshots have been dropped.
    pub fn snapshot(&self) -> Analysis {
        let snapshot_id = next_snapshot_id();
        Analysis {
            db: self.db.clone(),
            project_files: self.db.project_files_input,
            snapshot_id,
        }
    }
}

impl Default for AnalysisHost {
    fn default() -> Self {
        Self::new()
    }
}
