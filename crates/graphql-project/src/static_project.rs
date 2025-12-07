use crate::{
    Diagnostic, DocumentIndex, DocumentLoader, GraphQLConfig, ProjectConfig, Result, SchemaIndex,
    SchemaLoader, Validator,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Type alias for diagnostics grouped by file path
pub type DiagnosticsMap = HashMap<PathBuf, Vec<Diagnostic>>;

/// A static snapshot of a GraphQL project loaded from disk
///
/// Use this for:
/// - CLI validation commands
/// - CI/CD checks
/// - One-time analysis
/// - Code generation
///
/// Characteristics:
/// - Immutable after loading
/// - No caching overhead
/// - No dependency tracking
/// - Simple, predictable behavior
/// - Load → Validate → Report → Done
pub struct StaticGraphQLProject {
    config: ProjectConfig,
    #[allow(dead_code)]
    base_dir: Option<PathBuf>,
    schema_index: SchemaIndex,
    document_index: DocumentIndex,
}

impl StaticGraphQLProject {
    /// Create projects from GraphQL config with a base directory
    ///
    /// This is the main entry point for CLI tools. It loads all projects
    /// defined in the config file.
    pub async fn from_config_with_base(
        config: &GraphQLConfig,
        base_dir: &std::path::Path,
    ) -> Result<Vec<(String, Self)>> {
        // Collect project configs first to avoid Send issues with iterator
        let project_configs: Vec<(String, ProjectConfig)> = config
            .projects()
            .map(|(name, cfg)| (name.to_string(), cfg.clone()))
            .collect();

        let mut projects = Vec::new();
        for (name, project_config) in project_configs {
            let project = Self::load(project_config, Some(base_dir.to_path_buf())).await?;
            projects.push((name, project));
        }

        Ok(projects)
    }

    /// Create and load a static project from config
    ///
    /// This loads the entire project from disk:
    /// - All schema files
    /// - All document files (matching glob patterns)
    /// - Builds indices
    /// - Ready for validation
    pub async fn load(config: ProjectConfig, base_dir: Option<PathBuf>) -> Result<Self> {
        // Load schema
        let mut schema_loader = SchemaLoader::new(config.schema.clone());
        if let Some(ref base_path) = base_dir {
            schema_loader = schema_loader.with_base_path(base_path);
        }
        let schema_files = schema_loader.load_with_paths().await?;
        let schema_index = SchemaIndex::from_schema_files(schema_files);

        // Load documents (if configured)
        let document_index = if let Some(ref documents_config) = config.documents {
            let mut document_loader = DocumentLoader::new(documents_config.clone());
            if let Some(ref base_path) = base_dir {
                document_loader = document_loader.with_base_path(base_path);
            }
            document_loader.load()?
        } else {
            DocumentIndex::new()
        };

        Ok(Self {
            config,
            base_dir,
            schema_index,
            document_index,
        })
    }

    /// Validate the entire project
    ///
    /// Returns diagnostics for all files in the project.
    /// This runs:
    /// - Schema validation
    /// - Document validation (all documents)
    ///
    /// Note: Linting should be performed by the consumer (LSP/CLI) after calling this method.
    /// The consumer can use the graphql-linter crate with the schema and document indices.
    ///
    /// No caching, no incrementality - just straightforward validation.
    #[must_use]
    pub fn validate_all(&self) -> DiagnosticsMap {
        let all_diagnostics = HashMap::new();
        let _validator = Validator::new();

        // TODO: Implement validation logic
        // This requires either:
        // 1. Storing file contents in the DocumentIndex during load
        // 2. Re-reading files from disk here
        // 3. Using the parsed ASTs from DocumentIndex
        //
        // For now, return empty diagnostics.
        // The validation integration will be completed in a follow-up.

        all_diagnostics
    }

    /// Check if a file contains only fragment definitions (no operations)
    #[allow(dead_code)] // Will be used when validation is fully implemented
    fn is_fragment_only_file(&self, file_path: &std::path::Path) -> bool {
        let file_path_str = file_path.to_string_lossy();

        // Check if file has any operations
        let has_operations = self
            .document_index
            .operations
            .values()
            .any(|ops| ops.iter().any(|op| op.file_path == file_path_str));

        // Check if file has any fragments
        let has_fragments = self
            .document_index
            .fragments
            .values()
            .any(|frags| frags.iter().any(|frag| frag.file_path == file_path_str));

        // Fragment-only if it has fragments but no operations
        has_fragments && !has_operations
    }

    /// Get all files in the project (for reporting purposes)
    #[must_use]
    pub fn all_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Add document files from operations
        for operations in self.document_index.operations.values() {
            for op in operations {
                let path = PathBuf::from(&op.file_path);
                if seen.insert(path.clone()) {
                    files.push(path);
                }
            }
        }

        // Add document files from fragments
        for fragments in self.document_index.fragments.values() {
            for frag in fragments {
                let path = PathBuf::from(&frag.file_path);
                if seen.insert(path.clone()) {
                    files.push(path);
                }
            }
        }

        files
    }

    /// Get reference to schema index
    #[must_use]
    pub const fn schema_index(&self) -> &SchemaIndex {
        &self.schema_index
    }

    /// Get reference to document index
    #[must_use]
    pub const fn document_index(&self) -> &DocumentIndex {
        &self.document_index
    }

    /// Get reference to config
    #[must_use]
    pub const fn config(&self) -> &ProjectConfig {
        &self.config
    }
}
