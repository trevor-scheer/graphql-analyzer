//! CLI adapter for `AnalysisHost`
//!
//! This module provides a CLI-friendly wrapper around `graphql-ide::AnalysisHost`.
//! It handles batch loading from `GraphQLConfig` and provides conveniences for
//! collecting diagnostics across all project files.

use crate::schema_cache::SchemaCache;
use anyhow::{Context, Result};
use graphql_config::ProjectConfig;
use graphql_ide::{AnalysisHost, Diagnostic, FileKind, FilePath};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// CLI adapter for `AnalysisHost`
///
/// Wraps `graphql-ide::AnalysisHost` and provides CLI-specific conveniences:
/// - Batch loading from `GraphQLConfig`
/// - Collect all diagnostics across project
/// - Handle absolute file paths for CLI output
pub struct CliAnalysisHost {
    host: AnalysisHost,
    /// Track all loaded files for diagnostics collection
    loaded_files: Vec<PathBuf>,
}

impl CliAnalysisHost {
    /// Create from a project configuration
    ///
    /// Loads all schema and document files from the project config.
    /// If `refresh_schema` is true, remote schemas will be re-fetched even if cached.
    pub fn from_project_config(project_config: &ProjectConfig, base_dir: &Path) -> Result<Self> {
        Self::from_project_config_with_options(project_config, base_dir, false)
    }

    /// Create from a project configuration with cache options
    ///
    /// Loads all schema and document files from the project config.
    /// If `refresh_schema` is true, remote schemas will be re-fetched even if cached.
    pub fn from_project_config_with_options(
        project_config: &ProjectConfig,
        base_dir: &Path,
        refresh_schema: bool,
    ) -> Result<Self> {
        let mut host = AnalysisHost::new();
        let mut loaded_files = Vec::new();

        if let Some(ref lint_value) = project_config.lint {
            tracing::debug!("Raw lint configuration: {lint_value:?}");
            match serde_json::from_value::<graphql_linter::LintConfig>(lint_value.clone()) {
                Ok(lint_config) => {
                    if let Err(validation_error) = lint_config.validate() {
                        return Err(anyhow::anyhow!(
                            "Invalid lint configuration:\n\n{validation_error}"
                        ));
                    }

                    tracing::info!("Loaded lint configuration from project config");
                    tracing::debug!("Parsed lint config - unique_names enabled: {}, unused_fields enabled: {}, redundant_fields enabled: {}",
                        lint_config.is_enabled("unique_names"),
                        lint_config.is_enabled("unused_fields"),
                        lint_config.is_enabled("redundant_fields"));
                    host.set_lint_config(lint_config);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to parse lint configuration: {e}\n\n\
                         Expected format:\n\
                         lint:\n  \
                           rule_name: error  # or 'warn' or 'off'\n  \
                           another_rule: warn"
                    ));
                }
            }
        } else {
            tracing::debug!("No lint configuration found in project config, using defaults");
        }

        if let Some(ref extensions) = project_config.extensions {
            if let Some(extract_config_value) = extensions.get("extractConfig") {
                tracing::debug!("Raw extract configuration: {extract_config_value:?}");
                match serde_json::from_value::<graphql_extract::ExtractConfig>(
                    extract_config_value.clone(),
                ) {
                    Ok(extract_config) => {
                        tracing::info!("Loaded extract configuration from project config");
                        tracing::debug!(
                            allow_global_identifiers = extract_config.allow_global_identifiers,
                            tag_identifiers = ?extract_config.tag_identifiers,
                            "Parsed extract config"
                        );
                        host.set_extract_config(extract_config);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse extract configuration: {e}, using defaults"
                        );
                    }
                }
            }
        }

        let schema_result = host.load_schemas_from_config(project_config, base_dir)?;

        // Handle pending introspections (remote schemas)
        if !schema_result.pending_introspections.is_empty() {
            Self::fetch_remote_schemas(&mut host, &schema_result.pending_introspections, refresh_schema)?;
        }

        if let Some(ref documents_config) = project_config.documents {
            let document_files =
                Self::load_document_files(documents_config, base_dir, project_config)?;

            // Build batch of files with their kinds
            let files_to_add: Vec<(FilePath, String, FileKind)> = document_files
                .into_iter()
                .map(|(path, content)| {
                    let kind = match path.extension().and_then(|e| e.to_str()) {
                        Some("ts" | "tsx") => FileKind::TypeScript,
                        Some("js" | "jsx") => FileKind::JavaScript,
                        _ => FileKind::ExecutableGraphQL,
                    };
                    loaded_files.push(path.clone());
                    (
                        FilePath::new(path.to_string_lossy().to_string()),
                        content,
                        kind,
                    )
                })
                .collect();

            // Batch add all files for O(n) performance (instead of O(n²) with per-file add)
            let batch_refs: Vec<(FilePath, &str, FileKind)> = files_to_add
                .iter()
                .map(|(path, content, kind)| (path.clone(), content.as_str(), *kind))
                .collect();
            host.add_files_batch(&batch_refs);
        } else {
            // No documents to load, but still need to rebuild for schemas
            host.rebuild_project_files();
        }

        Ok(Self { host, loaded_files })
    }

    /// Load document files from config
    fn load_document_files(
        documents_config: &graphql_config::DocumentsConfig,
        base_dir: &Path,
        _project_config: &ProjectConfig,
    ) -> Result<Vec<(PathBuf, String)>> {
        // Use glob to match all document files
        // This ensures we load ALL matched files, even if they have parse errors
        let patterns: Vec<_> = documents_config.patterns().into_iter().collect();
        let mut file_paths = std::collections::HashSet::new();

        for pattern in patterns {
            let expanded = Self::expand_braces(pattern);

            for expanded_pattern in expanded {
                let full_pattern = base_dir.join(&expanded_pattern).display().to_string();

                for entry in glob::glob(&full_pattern)
                    .with_context(|| format!("Invalid glob pattern: {full_pattern}"))?
                {
                    match entry {
                        Ok(path) if path.is_file() => {
                            if path.components().any(|c| c.as_os_str() == "node_modules") {
                                continue;
                            }
                            file_paths.insert(path);
                        }
                        Ok(_) => {} // Skip directories
                        Err(e) => {
                            return Err(anyhow::anyhow!("Glob error: {e}"));
                        }
                    }
                }
            }
        }

        let mut files = Vec::new();
        for path in file_paths {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            files.push((path, content));
        }

        Ok(files)
    }

    /// Fetch remote schemas with caching support
    ///
    /// This method fetches remote schemas via introspection, using a cache to
    /// avoid redundant network requests. Cached schemas have a 1-hour TTL.
    fn fetch_remote_schemas(
        host: &mut AnalysisHost,
        pending: &[graphql_ide::PendingIntrospection],
        refresh: bool,
    ) -> Result<()> {
        // Initialize the cache (this is a no-op if it already exists)
        let cache = SchemaCache::new()
            .context("Failed to initialize schema cache")?;

        // We need to run async code from sync context
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create async runtime")?;

        for introspection in pending {
            let url = &introspection.url;

            // Check cache first (unless refresh is requested)
            if !refresh {
                if let Some((sdl, metadata)) = cache.get(url) {
                    tracing::info!(
                        url,
                        age_secs = metadata.age().as_secs(),
                        "Using cached schema"
                    );
                    host.add_introspected_schema(url, &sdl);
                    continue;
                }
            }

            // Fetch from remote
            tracing::info!(url, "Fetching remote schema");

            let sdl = runtime.block_on(async {
                let mut client = graphql_introspect::IntrospectionClient::new();

                // Apply headers if configured
                if let Some(ref headers) = introspection.headers {
                    for (name, value) in headers {
                        client = client.with_header(name, value);
                    }
                }

                // Apply timeout if configured
                if let Some(timeout_secs) = introspection.timeout {
                    client = client.with_timeout(std::time::Duration::from_secs(timeout_secs));
                }

                // Apply retries if configured
                if let Some(retries) = introspection.retry {
                    client = client.with_retries(retries);
                }

                let response = client.execute(url).await?;
                let sdl = graphql_introspect::introspection_to_sdl(&response);
                Ok::<_, anyhow::Error>(sdl)
            })?;

            // Cache the result
            if let Err(e) = cache.set(url, &sdl) {
                tracing::warn!(url, error = %e, "Failed to cache schema");
            }

            // Add to host
            host.add_introspected_schema(url, &sdl);
        }

        Ok(())
    }

    /// Expand brace patterns like "src/**/*.{ts,tsx}" into separate patterns
    fn expand_braces(pattern: &str) -> Vec<String> {
        // Simple brace expansion - handles single brace group
        if let Some(start) = pattern.find('{') {
            if let Some(end) = pattern.find('}') {
                let before = &pattern[..start];
                let after = &pattern[end + 1..];
                let options = &pattern[start + 1..end];

                return options
                    .split(',')
                    .map(|opt| format!("{}{}{}", before, opt.trim(), after))
                    .collect();
            }
        }

        vec![pattern.to_string()]
    }

    /// Get only validation diagnostics for all loaded files (excludes custom lint rules)
    ///
    /// Returns only GraphQL spec validation errors, not custom lint rule violations.
    /// Use this for the `validate` command to avoid duplicating lint checks.
    /// Only includes files that have diagnostics.
    pub fn all_validation_diagnostics(&self) -> HashMap<PathBuf, Vec<Diagnostic>> {
        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        for path in &self.loaded_files {
            let file_path = FilePath::new(path.to_string_lossy());
            let diagnostics = snapshot.validation_diagnostics(&file_path);

            if !diagnostics.is_empty() {
                results.insert(path.clone(), diagnostics);
            }
        }

        results
    }

    /// Get only lint diagnostics for all loaded files
    ///
    /// Returns only custom lint rule violations, excluding GraphQL spec validation errors.
    /// Includes both file-level and project-wide lint diagnostics.
    /// Only includes files that have diagnostics.
    pub fn all_lint_diagnostics(&self) -> HashMap<PathBuf, Vec<Diagnostic>> {
        tracing::info!(
            file_count = self.loaded_files.len(),
            "Starting lint diagnostics collection"
        );

        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        for (idx, path) in self.loaded_files.iter().enumerate() {
            tracing::debug!(
                file = %path.display(),
                progress = format!("{}/{}", idx + 1, self.loaded_files.len()),
                "Checking file for lint issues"
            );
            let file_path = FilePath::new(path.to_string_lossy());
            let diagnostics = snapshot.lint_diagnostics(&file_path);

            if !diagnostics.is_empty() {
                tracing::debug!(
                    file = %path.display(),
                    count = diagnostics.len(),
                    "File has lint issues"
                );
                results.insert(path.clone(), diagnostics);
            }
        }

        tracing::info!("Collecting project-wide lint diagnostics");
        let project_diagnostics = snapshot.project_lint_diagnostics();
        tracing::info!(
            file_count = project_diagnostics.len(),
            "Project-wide diagnostics collection complete"
        );
        for (file_path, diagnostics) in project_diagnostics {
            let path = PathBuf::from(file_path.as_str());
            if !diagnostics.is_empty() {
                tracing::debug!(
                    file = %path.display(),
                    count = diagnostics.len(),
                    "File has project-wide lint issues"
                );
            }
            results.entry(path).or_default().extend(diagnostics);
        }

        tracing::info!(
            total_files_with_issues = results.len(),
            "Lint diagnostics collection complete"
        );
        results
    }

    /// Get a snapshot of the analysis
    pub fn snapshot(&self) -> graphql_ide::Analysis {
        self.host.snapshot()
    }

    /// Get file count
    pub fn file_count(&self) -> usize {
        self.loaded_files.len()
    }

    /// Get schema statistics using HIR data
    pub fn schema_stats(&self) -> graphql_ide::SchemaStats {
        self.host.snapshot().schema_stats()
    }

    /// Get complexity statistics for operations
    pub fn complexity_stats(&self) -> (Vec<usize>, Vec<usize>) {
        use apollo_parser::Parser;
        use std::fs;

        let snapshot = self.host.snapshot();
        let mut depths = Vec::new();
        let mut usages = Vec::new();

        // Get all operations from workspace symbols
        let symbols = snapshot.workspace_symbols("");
        for symbol in &symbols {
            if matches!(
                symbol.kind,
                graphql_ide::SymbolKind::Query
                    | graphql_ide::SymbolKind::Mutation
                    | graphql_ide::SymbolKind::Subscription
            ) {
                // Read file content from disk to parse operations
                if let Ok(content) = fs::read_to_string(symbol.location.file.as_str()) {
                    let parser = Parser::new(&content);
                    let tree = parser.parse();

                    // Find the operation and count fragment spreads
                    for def in tree.document().definitions() {
                        use apollo_parser::cst::Definition;

                        if let Definition::OperationDefinition(op) = def {
                            if let Some(op_name) = op.name() {
                                if op_name.text() == symbol.name {
                                    // Count fragment spreads in this operation
                                    if let Some(selection_set) = op.selection_set() {
                                        let usage_count =
                                            count_fragment_spreads_in_selection_set(&selection_set);
                                        usages.push(usage_count);

                                        // Calculate depth (simple heuristic: just count 1 for now)
                                        depths.push(1);
                                    }
                                    break;
                                }
                            } else if symbol.name.is_empty() {
                                // Anonymous operation
                                if let Some(selection_set) = op.selection_set() {
                                    let usage_count =
                                        count_fragment_spreads_in_selection_set(&selection_set);
                                    usages.push(usage_count);
                                    depths.push(1);
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        (depths, usages)
    }

    /// Update a file (for watch mode - future enhancement)
    #[allow(dead_code)]
    pub fn update_file(&mut self, path: &Path, content: &str) {
        let file_path = FilePath::new(path.to_string_lossy().to_string());

        // Determine file kind based on whether it's in our loaded files
        // For simplicity, default to ExecutableGraphQL kind
        let kind = FileKind::ExecutableGraphQL;

        self.host.add_file(&file_path, content, kind);

        // Update loaded files list if this is a new file
        if !self.loaded_files.contains(&path.to_path_buf()) {
            self.loaded_files.push(path.to_path_buf());
        }
    }

    /// Get raw lint diagnostics with fix information for all loaded files
    ///
    /// This method returns `LintDiagnostic` objects that include fix information,
    /// which is needed for the `fix` command to apply auto-fixes.
    pub fn all_lint_diagnostics_with_fixes(
        &self,
    ) -> HashMap<PathBuf, Vec<graphql_linter::LintDiagnostic>> {
        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        // Get file-level lint diagnostics with fixes
        for path in &self.loaded_files {
            let file_path = FilePath::new(path.to_string_lossy().to_string());
            let diagnostics = snapshot.lint_diagnostics_with_fixes(&file_path);

            if !diagnostics.is_empty() {
                results.insert(path.clone(), diagnostics);
            }
        }

        // Get project-wide lint diagnostics with fixes
        let project_diagnostics = snapshot.project_lint_diagnostics_with_fixes();
        for (file_path, diagnostics) in project_diagnostics {
            let path = PathBuf::from(file_path.as_str());
            if !diagnostics.is_empty() {
                results.entry(path).or_default().extend(diagnostics);
            }
        }

        results
    }

    /// Get fragment usage analysis for the project
    ///
    /// Returns information about all fragments including:
    /// - Definition location
    /// - Usage count and locations
    /// - Transitive dependencies
    pub fn fragment_usages(&self) -> Vec<graphql_ide::FragmentUsage> {
        let snapshot = self.host.snapshot();
        snapshot.fragment_usages()
    }

    /// Get field coverage report for the project
    ///
    /// Returns coverage statistics showing which schema fields are used in operations.
    pub fn field_coverage(&self) -> Option<graphql_ide::FieldCoverageReport> {
        let snapshot = self.host.snapshot();
        snapshot.field_coverage()
    }

    /// Get complexity analysis for all operations in the project
    ///
    /// Analyzes each operation's selection set to calculate:
    /// - Total complexity score (with list multipliers)
    /// - Maximum depth
    /// - Per-field complexity breakdown
    /// - Warnings about potential issues (nested pagination, etc.)
    pub fn complexity_analysis(&self) -> Vec<graphql_ide::ComplexityAnalysis> {
        let snapshot = self.host.snapshot();
        snapshot.complexity_analysis()
    }
}

/// Count fragment spreads in a selection set
fn count_fragment_spreads_in_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
) -> usize {
    use apollo_parser::cst::Selection;

    let mut count = 0;

    for selection in selection_set.selections() {
        match selection {
            Selection::Field(field) => {
                // Recursively count in nested selection sets
                if let Some(nested) = field.selection_set() {
                    count += count_fragment_spreads_in_selection_set(&nested);
                }
            }
            Selection::FragmentSpread(_) => {
                // Found a fragment spread
                count += 1;
            }
            Selection::InlineFragment(inline) => {
                // Recursively count in inline fragments
                if let Some(nested) = inline.selection_set() {
                    count += count_fragment_spreads_in_selection_set(&nested);
                }
            }
        }
    }

    count
}
