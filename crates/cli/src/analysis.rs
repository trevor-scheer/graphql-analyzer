//! CLI adapter for `AnalysisHost`
//!
//! This module provides a CLI-friendly wrapper around `graphql-ide::AnalysisHost`.
//! It handles batch loading from `GraphQLConfig` and provides conveniences for
//! collecting diagnostics across all project files.

use anyhow::{Context, Result};
use graphql_config::ProjectConfig;
use graphql_ide::{AnalysisHost, Diagnostic, DocumentKind, FilePath, Language};
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
    /// Track schema files for diagnostics collection
    schema_files: Vec<PathBuf>,
    /// Track document files for diagnostics collection
    document_files: Vec<PathBuf>,
    /// Whether any user schema files were loaded (excludes builtins)
    schema_loaded: bool,
}

impl CliAnalysisHost {
    /// Create from a project configuration
    ///
    /// Loads all schema and document files from the project config.
    pub fn from_project_config(project_config: &ProjectConfig, base_dir: &Path) -> Result<Self> {
        let mut host = AnalysisHost::new();
        let mut schema_files = Vec::new();
        let mut document_files = Vec::new();

        if let Some(lint_value) = project_config.lint() {
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
                         extensions:\n  \
                           lint:\n    \
                             rules:\n      \
                               ruleName: error  # or 'warn' or 'off'"
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

        let schema_loaded = !schema_result.has_no_user_schema();

        // Check for content mismatch errors (schema files containing executable definitions)
        if !schema_result.content_errors.is_empty() {
            use std::fmt::Write;
            let mut error_msg = String::from("Content mismatch in schema configuration:\n");
            for error in &schema_result.content_errors {
                let _ = write!(
                    error_msg,
                    "\n  File '{}' matched by pattern '{}' contains executable definitions:\n",
                    error.file_path.display(),
                    error.pattern
                );
                for def in &error.unexpected_definitions {
                    let _ = writeln!(error_msg, "    - {def}");
                }
            }
            error_msg.push_str(
                "\nMove these files to the documents config or remove the executable definitions.",
            );
            return Err(anyhow::anyhow!("{error_msg}"));
        }

        schema_files.extend(schema_result.loaded_paths);

        if let Some(ref documents_config) = project_config.documents {
            let loaded_docs =
                Self::load_document_files(documents_config, base_dir, project_config)?;

            // Validate document files for schema definitions (content mismatch)
            let mut content_errors: Vec<(PathBuf, Vec<String>)> = Vec::new();
            for (path, content) in &loaded_docs {
                let (language, _) = match path.extension().and_then(|e| e.to_str()) {
                    Some("ts" | "tsx") => (Language::TypeScript, DocumentKind::Executable),
                    Some("js" | "jsx") => (Language::JavaScript, DocumentKind::Executable),
                    _ => (Language::GraphQL, DocumentKind::Executable),
                };

                // For TS/JS files, extract GraphQL first
                let graphql_content = if language.requires_extraction() {
                    let extract_config = graphql_extract::ExtractConfig::default();
                    graphql_extract::extract_from_source(content, language, &extract_config)
                        .unwrap_or_default()
                        .iter()
                        .map(|block| block.source.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    content.clone()
                };

                // Check for schema definitions in document files
                if let Some(mismatch) = graphql_syntax::validate_content_matches_kind(
                    &graphql_content,
                    DocumentKind::Executable,
                ) {
                    let definitions = match mismatch {
                        graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema {
                            definitions,
                        } => definitions,
                        graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable {
                            ..
                        } => Vec::new(),
                    };
                    if !definitions.is_empty() {
                        content_errors.push((path.clone(), definitions));
                    }
                }
            }

            // Fail fast if document files contain schema definitions
            if !content_errors.is_empty() {
                use std::fmt::Write;
                let mut error_msg = String::from("Content mismatch in documents configuration:\n");
                for (path, definitions) in &content_errors {
                    let _ = write!(
                        error_msg,
                        "\n  File '{}' contains schema definitions:\n",
                        path.display()
                    );
                    for def in definitions {
                        let _ = writeln!(error_msg, "    - {def}");
                    }
                }
                error_msg.push_str(
                    "\nMove these files to the schema config or remove the schema definitions.",
                );
                return Err(anyhow::anyhow!("{error_msg}"));
            }

            let files_to_add: Vec<(FilePath, String, Language, DocumentKind)> = loaded_docs
                .into_iter()
                .map(|(path, content)| {
                    let (language, document_kind) = match path.extension().and_then(|e| e.to_str())
                    {
                        Some("ts" | "tsx") => (Language::TypeScript, DocumentKind::Executable),
                        Some("js" | "jsx") => (Language::JavaScript, DocumentKind::Executable),
                        _ => (Language::GraphQL, DocumentKind::Executable),
                    };
                    document_files.push(path.clone());
                    (FilePath::from_path(&path), content, language, document_kind)
                })
                .collect();

            // Batch add all files for O(n) performance (instead of O(n²) with per-file add)
            let batch_refs: Vec<(FilePath, &str, Language, DocumentKind)> = files_to_add
                .iter()
                .map(|(path, content, language, document_kind)| {
                    (path.clone(), content.as_str(), *language, *document_kind)
                })
                .collect();
            host.add_files_batch(&batch_refs);
        } else {
            // No documents to load, but still need to rebuild for schemas
            host.rebuild_project_files();
        }

        Ok(Self {
            host,
            schema_files,
            document_files,
            schema_loaded,
        })
    }

    /// Load document files from config
    fn load_document_files(
        documents_config: &graphql_config::DocumentsConfig,
        base_dir: &Path,
        _project_config: &ProjectConfig,
    ) -> Result<Vec<(PathBuf, String)>> {
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
                        Ok(_) => {}
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
    ///
    /// Schema errors are filtered to only show errors that originate from each file.
    /// Document errors are collected per file.
    pub fn all_validation_diagnostics(&self) -> HashMap<PathBuf, Vec<Diagnostic>> {
        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        for path in &self.schema_files {
            let file_path = FilePath::from_path(path);
            let diagnostics = snapshot.validation_diagnostics(&file_path);

            if !diagnostics.is_empty() {
                results.insert(path.clone(), diagnostics);
            }
        }

        for path in &self.document_files {
            let file_path = FilePath::from_path(path);
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
        let total_files = self.schema_files.len() + self.document_files.len();
        tracing::info!(
            file_count = total_files,
            "Starting lint diagnostics collection"
        );

        let snapshot = self.host.snapshot();
        let mut results = HashMap::new();

        // Lint document files (schema files don't have lint rules)
        for (idx, path) in self.document_files.iter().enumerate() {
            tracing::debug!(
                file = %path.display(),
                progress = format!("{}/{}", idx + 1, self.document_files.len()),
                "Checking file for lint issues"
            );
            let file_path = FilePath::from_path(path);
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

    /// Returns true if user schema files were loaded (excludes Apollo Client builtins).
    pub fn schema_loaded(&self) -> bool {
        self.schema_loaded
    }

    /// Get file count
    pub fn file_count(&self) -> usize {
        self.schema_files.len() + self.document_files.len()
    }

    /// Returns the number of document files loaded.
    pub fn document_count(&self) -> usize {
        self.document_files.len()
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

    /// Update a file in the analysis host (used by watch mode)
    pub fn update_file(
        &mut self,
        path: &Path,
        content: &str,
        language: Language,
        document_kind: DocumentKind,
    ) {
        let file_path = FilePath::from_path(path);
        self.host
            .add_file(&file_path, content, language, document_kind);

        // Update document files list if this is a new file
        let path_buf = path.to_path_buf();
        if !self.document_files.contains(&path_buf) && !self.schema_files.contains(&path_buf) {
            self.document_files.push(path_buf);
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
        for path in &self.document_files {
            let file_path = FilePath::from_path(path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_braces_single_brace_group() {
        let result = CliAnalysisHost::expand_braces("src/**/*.{ts,tsx}");
        assert_eq!(result, vec!["src/**/*.ts", "src/**/*.tsx"]);
    }

    #[test]
    fn test_expand_braces_three_options() {
        let result = CliAnalysisHost::expand_braces("**/*.{js,jsx,ts}");
        assert_eq!(result, vec!["**/*.js", "**/*.jsx", "**/*.ts"]);
    }

    #[test]
    fn test_expand_braces_no_braces() {
        let result = CliAnalysisHost::expand_braces("src/**/*.graphql");
        assert_eq!(result, vec!["src/**/*.graphql"]);
    }

    #[test]
    fn test_expand_braces_with_spaces() {
        let result = CliAnalysisHost::expand_braces("src/**/*.{ts, tsx}");
        assert_eq!(result, vec!["src/**/*.ts", "src/**/*.tsx"]);
    }

    #[test]
    fn test_expand_braces_single_option() {
        let result = CliAnalysisHost::expand_braces("src/**/*.{graphql}");
        assert_eq!(result, vec!["src/**/*.graphql"]);
    }

    #[test]
    fn test_schema_file_with_executable_definitions_fails() {
        use graphql_config::{ProjectConfig, SchemaConfig};
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create a schema directory with a file containing executable definitions
        let schema_dir = workspace_path.join("schema");
        std::fs::create_dir(&schema_dir).unwrap();

        let mut schema_file = std::fs::File::create(schema_dir.join("bad.graphql")).unwrap();
        writeln!(
            schema_file,
            "# This file has executable definitions in schema config"
        )
        .unwrap();
        writeln!(schema_file, "query GetUser {{ user {{ id }} }}").unwrap();
        writeln!(schema_file, "fragment UserFields on User {{ id name }}").unwrap();

        let project_config = ProjectConfig {
            schema: SchemaConfig::Path("schema/*.graphql".to_string()),
            documents: None,
            include: None,
            exclude: None,
            extensions: None,
        };

        let result = CliAnalysisHost::from_project_config(&project_config, workspace_path);

        // Should fail with a content mismatch error
        assert!(result.is_err());
        let error = result.err().unwrap();
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("Content mismatch"),
            "Expected 'Content mismatch' in error: {error_msg}"
        );
        assert!(
            error_msg.contains("executable definitions"),
            "Expected 'executable definitions' in error: {error_msg}"
        );
    }

    #[test]
    fn test_document_file_with_schema_definitions_fails() {
        use graphql_config::{DocumentsConfig, ProjectConfig, SchemaConfig};
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create a valid schema file
        let schema_dir = workspace_path.join("schema");
        std::fs::create_dir(&schema_dir).unwrap();
        let mut schema_file = std::fs::File::create(schema_dir.join("schema.graphql")).unwrap();
        writeln!(schema_file, "type Query {{ user: User }}").unwrap();
        writeln!(schema_file, "type User {{ id: ID! }}").unwrap();

        // Create a documents directory with a file containing schema definitions
        let docs_dir = workspace_path.join("docs");
        std::fs::create_dir(&docs_dir).unwrap();

        let mut bad_doc_file = std::fs::File::create(docs_dir.join("bad.graphql")).unwrap();
        writeln!(
            bad_doc_file,
            "# This file has schema definitions in documents config"
        )
        .unwrap();
        writeln!(bad_doc_file, "type BadType {{ id: ID! }}").unwrap();
        writeln!(bad_doc_file, "interface BadInterface {{ name: String }}").unwrap();

        let project_config = ProjectConfig {
            schema: SchemaConfig::Path("schema/*.graphql".to_string()),
            documents: Some(DocumentsConfig::Pattern("docs/*.graphql".to_string())),
            include: None,
            exclude: None,
            extensions: None,
        };

        let result = CliAnalysisHost::from_project_config(&project_config, workspace_path);

        // Should fail with a content mismatch error
        assert!(result.is_err());
        let error = result.err().unwrap();
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("Content mismatch"),
            "Expected 'Content mismatch' in error: {error_msg}"
        );
        assert!(
            error_msg.contains("schema definitions"),
            "Expected 'schema definitions' in error: {error_msg}"
        );
    }

    #[test]
    fn test_valid_files_load_successfully() {
        use graphql_config::{DocumentsConfig, ProjectConfig, SchemaConfig};
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create a valid schema file
        let schema_dir = workspace_path.join("schema");
        std::fs::create_dir(&schema_dir).unwrap();
        let mut schema_file = std::fs::File::create(schema_dir.join("schema.graphql")).unwrap();
        writeln!(schema_file, "type Query {{ user: User }}").unwrap();
        writeln!(schema_file, "type User {{ id: ID!, name: String }}").unwrap();

        // Create a valid document file
        let docs_dir = workspace_path.join("docs");
        std::fs::create_dir(&docs_dir).unwrap();
        let mut doc_file = std::fs::File::create(docs_dir.join("queries.graphql")).unwrap();
        writeln!(doc_file, "query GetUser {{ user {{ id }} }}").unwrap();
        writeln!(doc_file, "fragment UserFields on User {{ id name }}").unwrap();

        let project_config = ProjectConfig {
            schema: SchemaConfig::Path("schema/*.graphql".to_string()),
            documents: Some(DocumentsConfig::Pattern("docs/*.graphql".to_string())),
            include: None,
            exclude: None,
            extensions: None,
        };

        let result = CliAnalysisHost::from_project_config(&project_config, workspace_path);

        // Should succeed
        assert!(
            result.is_ok(),
            "Expected success but got error: {}",
            result.err().unwrap()
        );
    }

    #[test]
    fn test_schema_loaded_true_when_schema_files_exist() {
        use graphql_config::{ProjectConfig, SchemaConfig};
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let schema_dir = workspace_path.join("schema");
        std::fs::create_dir(&schema_dir).unwrap();
        let mut schema_file = std::fs::File::create(schema_dir.join("schema.graphql")).unwrap();
        writeln!(schema_file, "type Query {{ hello: String }}").unwrap();

        let project_config = ProjectConfig {
            schema: SchemaConfig::Path("schema/*.graphql".to_string()),
            documents: None,
            include: None,
            exclude: None,
            extensions: None,
        };

        let host = CliAnalysisHost::from_project_config(&project_config, workspace_path).unwrap();
        assert!(host.schema_loaded());
    }

    #[test]
    fn test_schema_loaded_false_when_no_schema_files_match() {
        use graphql_config::{ProjectConfig, SchemaConfig};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Point at a pattern that matches nothing
        let project_config = ProjectConfig {
            schema: SchemaConfig::Path("nonexistent/*.graphql".to_string()),
            documents: None,
            include: None,
            exclude: None,
            extensions: None,
        };

        let host = CliAnalysisHost::from_project_config(&project_config, workspace_path).unwrap();
        assert!(!host.schema_loaded());
    }
}
