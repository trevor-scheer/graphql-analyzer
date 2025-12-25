//! CLI adapter for `AnalysisHost`
//!
//! This module provides a CLI-friendly wrapper around `graphql-ide::AnalysisHost`.
//! It handles batch loading from `GraphQLConfig` and provides conveniences for
//! collecting diagnostics across all project files.

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
    #[allow(dead_code)]
    base_dir: PathBuf,
}

impl CliAnalysisHost {
    /// Create from a project configuration
    ///
    /// Loads all schema and document files from the project config.
    pub async fn from_project_config(
        project_config: &ProjectConfig,
        base_dir: PathBuf,
    ) -> Result<Self> {
        let mut host = AnalysisHost::new();
        let mut loaded_files = Vec::new();

        // Load schema files
        let schema_files = Self::load_schema_files(project_config, &base_dir).await?;

        for (path, content) in schema_files {
            host.add_file(
                &FilePath::new(path.to_string_lossy().to_string()),
                &content,
                FileKind::Schema,
                0, // No line offset for pure GraphQL files
            );
            loaded_files.push(path);
        }

        // Load document files (if configured)
        if let Some(ref documents_config) = project_config.documents {
            let document_files =
                Self::load_document_files(documents_config, &base_dir, project_config)?;

            for (path, content) in document_files {
                // Determine file kind based on extension
                let kind = match path.extension().and_then(|e| e.to_str()) {
                    Some("ts" | "tsx") => FileKind::TypeScript,
                    Some("js" | "jsx") => FileKind::JavaScript,
                    _ => FileKind::ExecutableGraphQL, // .graphql, .gql, or unknown
                };

                host.add_file(
                    &FilePath::new(path.to_string_lossy().to_string()),
                    &content,
                    kind,
                    0, // No line offset for pure GraphQL files from disk
                );
                loaded_files.push(path);
            }
        }

        Ok(Self {
            host,
            loaded_files,
            base_dir,
        })
    }

    /// Load schema files from config
    async fn load_schema_files(
        config: &ProjectConfig,
        base_dir: &Path,
    ) -> Result<Vec<(PathBuf, String)>> {
        use graphql_project::SchemaLoader;

        let mut loader = SchemaLoader::new(config.schema.clone());
        loader = loader.with_base_path(base_dir);

        let schema_files = loader
            .load_with_paths()
            .await
            .context("Failed to load schema files")?;

        // Convert String paths to PathBuf
        Ok(schema_files
            .into_iter()
            .map(|(path, content)| (PathBuf::from(path), content))
            .collect())
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
            // Expand brace patterns like {ts,tsx}
            let expanded = Self::expand_braces(pattern);

            for expanded_pattern in expanded {
                let full_pattern = base_dir.join(&expanded_pattern).display().to_string();

                for entry in glob::glob(&full_pattern)
                    .with_context(|| format!("Invalid glob pattern: {full_pattern}"))?
                {
                    match entry {
                        Ok(path) if path.is_file() => {
                            // Skip node_modules
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

        // Read file contents
        let mut files = Vec::new();
        for path in file_paths {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            files.push((path, content));
        }

        Ok(files)
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

    /// Get diagnostics for all loaded files
    ///
    /// Returns a map of file path -> diagnostics.
    /// Only includes files that have diagnostics.
    pub fn all_diagnostics(&self) -> HashMap<PathBuf, Vec<Diagnostic>> {
        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        for path in &self.loaded_files {
            let file_path = FilePath::new(path.to_string_lossy().to_string());
            let diagnostics = snapshot.diagnostics(&file_path);

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
        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        // Get file-level lint diagnostics
        for path in &self.loaded_files {
            let file_path = FilePath::new(path.to_string_lossy().to_string());
            let diagnostics = snapshot.lint_diagnostics(&file_path);

            if !diagnostics.is_empty() {
                results.insert(path.clone(), diagnostics);
            }
        }

        // Get project-wide lint diagnostics (e.g., unused fields, unique names)
        let project_diagnostics = snapshot.project_lint_diagnostics();
        for (file_path, diagnostics) in project_diagnostics {
            let path = PathBuf::from(file_path.as_str());
            results.entry(path).or_default().extend(diagnostics);
        }

        results
    }

    /// Get the number of schema files loaded
    #[allow(dead_code)]
    pub fn schema_file_count(&self) -> usize {
        // For now, approximate based on file extensions
        // In a full implementation, we'd track this separately
        self.loaded_files
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "graphql" | "gql" | "graphqls"))
            })
            .count()
    }

    /// Get the number of operation and fragment definitions
    ///
    /// This queries the HIR layer to count operations and fragments.
    #[allow(dead_code)]
    pub fn definition_counts(&self) -> (usize, usize) {
        let _snapshot = self.host.snapshot();

        // Count operations and fragments across all files
        // For simplicity, we'll count from the loaded files
        // In the full implementation, we'd query the HIR layer

        // For now, return approximate counts
        // A full implementation would use graphql-hir queries
        (0, 0) // TODO: Query HIR for actual counts
    }

    /// Update a file (for watch mode - future enhancement)
    #[allow(dead_code)]
    pub fn update_file(&mut self, path: &Path, content: &str) {
        let file_path = FilePath::new(path.to_string_lossy().to_string());

        // Determine file kind based on whether it's in our loaded files
        // For simplicity, default to ExecutableGraphQL kind
        let kind = FileKind::ExecutableGraphQL;

        self.host.add_file(&file_path, content, kind, 0);

        // Update loaded files list if this is a new file
        if !self.loaded_files.contains(&path.to_path_buf()) {
            self.loaded_files.push(path.to_path_buf());
        }
    }
}
