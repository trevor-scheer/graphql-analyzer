use crate::{
    Diagnostic, DocumentIndex, DocumentLoader, ProjectConfig, Result, SchemaIndex, SchemaLoader,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Type alias for diagnostics grouped by file path
pub type DiagnosticsMap = HashMap<PathBuf, Vec<Diagnostic>>;

/// Validation mode determines how much work to do
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Quick: validate only the changed file
    /// Use for: real-time feedback during typing
    Quick,

    /// Smart: validate changed file + affected files
    /// Use for: `did_save`, after debounce
    /// Example affected files:
    /// - Files using fragments defined in changed file
    /// - Files with duplicate names
    /// - Schema files if field usage changed
    Smart,

    /// Full: validate entire project
    /// Use for: schema changes, CLI validation
    Full,
}

/// Tracks dependencies between files in the project
pub struct DependencyGraph {
    /// Fragment name -> files that define it
    fragment_definitions: HashMap<String, HashSet<PathBuf>>,

    /// Fragment name -> files that use it
    fragment_usages: HashMap<String, HashSet<PathBuf>>,

    /// Operation name -> file that defines it
    operation_definitions: HashMap<String, PathBuf>,

    /// Type name -> files using it (for schema changes)
    /// TODO: Implement tracking once `OperationInfo` has field selection tracking
    #[allow(dead_code)]
    type_usages: HashMap<String, HashSet<PathBuf>>,

    /// Schema files (any schema change affects all documents)
    schema_files: HashSet<PathBuf>,

    /// All indexed document files
    document_files: HashSet<PathBuf>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    #[must_use]
    pub fn new() -> Self {
        Self {
            fragment_definitions: HashMap::new(),
            fragment_usages: HashMap::new(),
            operation_definitions: HashMap::new(),
            type_usages: HashMap::new(),
            schema_files: HashSet::new(),
            document_files: HashSet::new(),
        }
    }

    /// Build a dependency graph from schema and document indices
    #[must_use]
    pub fn build(_schema_index: &SchemaIndex, document_index: &DocumentIndex) -> Self {
        let mut graph = Self::new();

        // Track fragment definitions
        for (fragment_name, fragment_info) in &document_index.fragments {
            for fragment in fragment_info {
                let file_path = PathBuf::from(&fragment.file_path);

                // Track fragment definition
                graph
                    .fragment_definitions
                    .entry(fragment_name.clone())
                    .or_default()
                    .insert(file_path.clone());

                // Track that this file exists
                graph.document_files.insert(file_path);
            }
        }

        // Track operation definitions
        for operations in document_index.operations.values() {
            for operation in operations {
                if let Some(name) = &operation.name {
                    let file_path = PathBuf::from(&operation.file_path);

                    // Track operation definition
                    graph
                        .operation_definitions
                        .insert(name.clone(), file_path.clone());

                    // Track that this file exists
                    graph.document_files.insert(file_path.clone());

                    // TODO: Track fragment usages once OperationInfo has fragment_spreads field
                }
            }
        }

        graph
    }

    /// Get all files affected by a change to the given file
    #[must_use]
    pub fn get_affected_files(&self, changed_file: &PathBuf) -> HashSet<PathBuf> {
        let mut affected = HashSet::new();

        // If it's a schema file, ALL documents are affected
        if self.schema_files.contains(changed_file) {
            affected.extend(self.document_files.iter().cloned());
            return affected;
        }

        // Check if this file defines fragments that others use
        for (frag_name, def_files) in &self.fragment_definitions {
            if def_files.contains(changed_file) {
                if let Some(users) = self.fragment_usages.get(frag_name) {
                    affected.extend(users.iter().cloned());
                }
            }
        }

        // Also include the file itself
        affected.insert(changed_file.clone());

        affected
    }

    /// Get fragment count for verification
    #[must_use]
    pub fn fragment_count(&self) -> usize {
        self.fragment_definitions.len()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A long-lived GraphQL project that handles incremental updates
///
/// Use this for:
/// - Language Server Protocol (LSP)
/// - Watch mode
/// - Interactive development tools
///
/// Characteristics:
/// - Mutable (files can be added/updated/removed)
/// - Tracks dependencies between files
/// - Caches validation results
/// - Smart revalidation (only affected files)
/// - Thread-safe (Arc/RwLock for concurrent access)
pub struct DynamicGraphQLProject {
    config: ProjectConfig,
    base_dir: Option<PathBuf>,

    // Thread-safe mutable state
    schema_index: Arc<RwLock<SchemaIndex>>,
    document_index: Arc<RwLock<DocumentIndex>>,

    // Dependency tracking for smart revalidation
    dependencies: Arc<RwLock<DependencyGraph>>,

    // Cached project-wide lint diagnostics per file
    // These are expensive to compute and only change when other files change
    project_lint_cache: Arc<RwLock<HashMap<PathBuf, Vec<Diagnostic>>>>,
}

impl DynamicGraphQLProject {
    /// Create a new dynamic project (empty, not loaded)
    #[must_use]
    pub fn new(config: ProjectConfig, base_dir: Option<PathBuf>) -> Self {
        Self {
            config,
            base_dir,
            schema_index: Arc::new(RwLock::new(SchemaIndex::new())),
            document_index: Arc::new(RwLock::new(DocumentIndex::new())),
            dependencies: Arc::new(RwLock::new(DependencyGraph::new())),
            project_lint_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize by loading all files from disk
    ///
    /// This is typically called once when the LSP starts up.
    /// Returns diagnostics for all loaded files.
    pub async fn initialize(&mut self) -> Result<DiagnosticsMap> {
        // Load schema
        self.load_schema().await?;

        // Load documents
        self.load_documents()?;

        // Build initial dependency graph
        self.rebuild_dependency_graph()?;

        // Validate everything
        self.validate_all_files(ValidationMode::Full).await
    }

    /// Load schema from configured sources
    #[allow(clippy::significant_drop_tightening)]
    async fn load_schema(&self) -> Result<()> {
        let mut schema_loader = SchemaLoader::new(self.config.schema.clone());
        if let Some(ref base_path) = self.base_dir {
            schema_loader = schema_loader.with_base_path(base_path);
        }
        let schema_files = schema_loader.load_with_paths().await?;
        let index = SchemaIndex::from_schema_files(schema_files);

        let mut schema_index = self.schema_index.write().unwrap();
        *schema_index = index;

        Ok(())
    }

    /// Load documents from configured sources
    #[allow(clippy::significant_drop_tightening)]
    fn load_documents(&self) -> Result<()> {
        let index = if let Some(ref documents_config) = self.config.documents {
            let mut document_loader = DocumentLoader::new(documents_config.clone());
            if let Some(ref base_path) = self.base_dir {
                document_loader = document_loader.with_base_path(base_path);
            }
            document_loader.load()?
        } else {
            DocumentIndex::new()
        };

        let mut document_index = self.document_index.write().unwrap();
        *document_index = index;

        Ok(())
    }

    /// Rebuild the entire dependency graph from current indices
    #[allow(clippy::significant_drop_tightening, clippy::unnecessary_wraps)]
    fn rebuild_dependency_graph(&self) -> Result<()> {
        let schema_index = self.schema_index.read().unwrap();
        let document_index = self.document_index.read().unwrap();

        let new_graph = DependencyGraph::build(&schema_index, &document_index);

        let mut deps = self.dependencies.write().unwrap();
        *deps = new_graph;

        tracing::debug!("Rebuilt dependency graph");
        Ok(())
    }

    /// Validate all files in the project
    #[allow(clippy::unused_async)] // Will be async when implemented
    async fn validate_all_files(&self, _mode: ValidationMode) -> Result<DiagnosticsMap> {
        // TODO: Implement validation
        Ok(HashMap::new())
    }

    /// Update or add a document (in-memory content)
    ///
    /// Returns diagnostics for this file AND all affected files.
    #[allow(clippy::unused_async)] // Will be async when implemented
    pub async fn add_or_update_document(
        &mut self,
        _file_path: PathBuf,
        _content: String,
        _mode: ValidationMode,
    ) -> Result<DiagnosticsMap> {
        // TODO: Implement
        Ok(HashMap::new())
    }

    /// Remove a document from the project
    #[allow(clippy::unused_async)] // Will be async when implemented
    pub async fn remove_document(&mut self, _file_path: PathBuf) -> Result<DiagnosticsMap> {
        // TODO: Implement
        Ok(HashMap::new())
    }

    /// Get cached diagnostics for a file (if available)
    #[must_use]
    pub fn get_diagnostics(&self, file_path: &PathBuf) -> Option<Vec<Diagnostic>> {
        self.project_lint_cache
            .read()
            .unwrap()
            .get(file_path)
            .cloned()
    }

    /// Get all cached diagnostics
    #[must_use]
    pub fn get_all_diagnostics(&self) -> DiagnosticsMap {
        self.project_lint_cache.read().unwrap().clone()
    }

    /// Get reference to schema index (for language features)
    #[must_use]
    pub fn schema_index(&self) -> Arc<RwLock<SchemaIndex>> {
        Arc::clone(&self.schema_index)
    }

    /// Get reference to document index (for language features)
    #[must_use]
    pub fn document_index(&self) -> Arc<RwLock<DocumentIndex>> {
        Arc::clone(&self.document_index)
    }

    /// Get reference to config
    #[must_use]
    pub const fn config(&self) -> &ProjectConfig {
        &self.config
    }
}
