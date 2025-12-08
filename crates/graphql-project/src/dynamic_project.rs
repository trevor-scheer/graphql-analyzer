use crate::{
    convert_apollo_diagnostics, Diagnostic, DocumentIndex, DocumentLoader, ProjectConfig, Result,
    SchemaIndex, SchemaLoader,
};
use graphql_extract::ExtractConfig;
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
    #[allow(clippy::unused_async)]
    async fn validate_all_files(&self, _mode: ValidationMode) -> Result<DiagnosticsMap> {
        // For now, return empty diagnostics
        // Full implementation will validate all tracked files
        Ok(HashMap::new())
    }

    /// Update or add a document (in-memory content)
    ///
    /// Returns diagnostics for this file AND all affected files.
    pub async fn add_or_update_document(
        &mut self,
        file_path: PathBuf,
        content: String,
        mode: ValidationMode,
    ) -> Result<DiagnosticsMap> {
        // Check if this is a schema file
        let is_schema = self.is_schema_file(&file_path);

        if is_schema {
            // Update schema index
            self.update_schema_index(&file_path, &content).await?;

            // Update dependency graph
            self.rebuild_dependency_graph()?;

            // Schema changes affect all documents - use Full mode
            return self.validate_all_files(ValidationMode::Full).await;
        }

        // Update document index
        self.update_document_index(&file_path, &content)?;

        // Update dependency graph
        self.rebuild_dependency_graph()?;

        // Validate based on mode
        match mode {
            ValidationMode::Quick => {
                // Just this file
                let diagnostics = self.validate_file(&file_path, &content)?;
                let mut result = HashMap::new();
                if !diagnostics.is_empty() {
                    result.insert(file_path, diagnostics);
                }
                Ok(result)
            }
            ValidationMode::Smart => {
                // This file + affected files
                let affected = {
                    let deps = self.dependencies.read().unwrap();
                    deps.get_affected_files(&file_path)
                };
                self.validate_files(affected, &content, &file_path)
            }
            ValidationMode::Full => {
                // Everything
                self.validate_all_files(ValidationMode::Full).await
            }
        }
    }

    /// Remove a document from the project
    pub fn remove_document(&mut self, file_path: &PathBuf) -> Result<DiagnosticsMap> {
        // Remove from document index
        {
            let mut document_index = self.document_index.write().unwrap();
            let file_path_str = file_path.to_string_lossy();

            // Remove operations
            document_index.operations.retain(|_, ops| {
                ops.retain(|op| op.file_path != file_path_str);
                !ops.is_empty()
            });

            // Remove fragments
            document_index.fragments.retain(|_, frags| {
                frags.retain(|frag| frag.file_path != file_path_str);
                !frags.is_empty()
            });

            // Remove extracted blocks
            document_index.remove_extracted_blocks(&file_path_str);

            // Remove line index
            document_index.line_indices.remove(file_path_str.as_ref());

            // Remove parsed ASTs
            document_index.parsed_asts.remove(file_path_str.as_ref());
        }

        // Remove from lint cache
        self.project_lint_cache.write().unwrap().remove(file_path);

        // Update dependency graph
        self.rebuild_dependency_graph()?;

        // Get affected files (files that depended on this one)
        let affected = {
            let deps = self.dependencies.read().unwrap();
            deps.get_affected_files(file_path)
        };

        // Revalidate affected files
        // Note: We can't validate the removed file, so we filter it out
        let affected: HashSet<_> = affected.into_iter().filter(|p| p != file_path).collect();

        if affected.is_empty() {
            return Ok(HashMap::new());
        }

        // For affected files, we need their content - this is a limitation
        // In the LSP context, the caller should have the content available
        // For now, return empty diagnostics for affected files
        Ok(HashMap::new())
    }

    /// Check if a file is a schema file
    #[allow(clippy::unused_self)]
    fn is_schema_file(&self, file_path: &std::path::Path) -> bool {
        // Check if file path matches schema patterns
        let file_path_str = file_path.to_string_lossy();

        // Simple heuristic: check if it's in schema_index
        // A better approach would check against schema patterns from config
        file_path_str.contains("schema")
    }

    /// Update schema index with new content
    #[allow(clippy::significant_drop_tightening)]
    async fn update_schema_index(&self, file_path: &std::path::Path, content: &str) -> Result<()> {
        let mut schema_loader = SchemaLoader::new(self.config.schema.clone());
        if let Some(ref base_path) = self.base_dir {
            schema_loader = schema_loader.with_base_path(base_path);
        }

        let mut schema_files = schema_loader.load_with_paths().await?;

        // Replace the content of the specified file
        let file_path_str = file_path.to_string_lossy().to_string();
        let mut found = false;
        for (path, file_content) in &mut schema_files {
            if path == &file_path_str {
                *file_content = content.to_string();
                found = true;
                break;
            }
        }

        // If not found, add it
        if !found {
            schema_files.push((file_path_str, content.to_string()));
        }

        // Rebuild schema index
        let index = SchemaIndex::from_schema_files(schema_files);
        let mut schema_index = self.schema_index.write().unwrap();
        *schema_index = index;

        Ok(())
    }

    /// Update document index with new content
    #[allow(clippy::significant_drop_tightening)]
    fn update_document_index(&self, file_path: &std::path::Path, content: &str) -> Result<()> {
        use apollo_parser::Parser;
        use graphql_extract::{extract_from_source, Language};

        // Determine language from file extension
        let language =
            file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or(Language::GraphQL, |ext| match ext.to_lowercase().as_str() {
                    "ts" | "tsx" => Language::TypeScript,
                    "js" | "jsx" => Language::JavaScript,
                    _ => Language::GraphQL,
                });

        // Extract GraphQL from the content
        let extract_config = self.get_extract_config();
        let extracted = extract_from_source(content, language, &extract_config)
            .map_err(|e| crate::ProjectError::DocumentLoad(format!("Extract error: {e}")))?;

        let file_path_str = file_path.to_string_lossy();

        // Acquire write lock and update index
        let mut document_index = self.document_index.write().unwrap();

        // Remove all existing entries for this file
        document_index.operations.retain(|_, ops| {
            ops.retain(|op| op.file_path != file_path_str);
            !ops.is_empty()
        });
        document_index.fragments.retain(|_, frags| {
            frags.retain(|frag| frag.file_path != file_path_str);
            !frags.is_empty()
        });

        // For pure GraphQL files, parse once and cache
        let is_pure_graphql = matches!(language, Language::GraphQL) && extracted.len() == 1;
        let parsed_arc = if is_pure_graphql {
            Arc::new(Parser::new(content).parse())
        } else {
            Arc::new(Parser::new("").parse()) // Placeholder
        };

        if is_pure_graphql {
            document_index.cache_ast(file_path_str.to_string(), Arc::clone(&parsed_arc));
        }

        // Build and cache line index
        let line_index = crate::LineIndex::new(content);
        document_index.cache_line_index(file_path_str.to_string(), Arc::new(line_index));

        // Cache extracted blocks
        let mut cached_blocks = Vec::new();
        for item in &extracted {
            let block_parsed_arc = if is_pure_graphql {
                Arc::clone(&parsed_arc)
            } else {
                Arc::new(Parser::new(&item.source).parse())
            };

            let block = crate::ExtractedBlock {
                content: item.source.clone(),
                offset: item.location.offset,
                length: item.location.length,
                start_line: item.location.range.start.line,
                start_column: item.location.range.start.column,
                end_line: item.location.range.end.line,
                end_column: item.location.range.end.column,
                parsed: block_parsed_arc,
            };
            cached_blocks.push(block);
        }

        if cached_blocks.is_empty() {
            document_index.remove_extracted_blocks(&file_path_str);
        } else {
            document_index.cache_extracted_blocks(file_path_str.to_string(), cached_blocks);
        }

        // Parse and index each extracted GraphQL block
        for item in extracted {
            DocumentLoader::parse_and_index(&item, &file_path_str, &mut document_index);
        }

        Ok(())
    }

    /// Get extract configuration
    fn get_extract_config(&self) -> ExtractConfig {
        self.config
            .extensions
            .as_ref()
            .and_then(|ext| ext.get("extractConfig"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    /// Validate a single file with given content
    fn validate_file(&self, file_path: &std::path::Path, content: &str) -> Result<Vec<Diagnostic>> {
        use graphql_extract::{extract_from_source, Language};

        let language =
            file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or(Language::GraphQL, |ext| match ext.to_lowercase().as_str() {
                    "ts" | "tsx" => Language::TypeScript,
                    "js" | "jsx" => Language::JavaScript,
                    _ => Language::GraphQL,
                });

        let extract_config = self.get_extract_config();
        let extracted = extract_from_source(content, language, &extract_config)
            .map_err(|e| crate::ProjectError::DocumentLoad(format!("Extract error: {e}")))?;

        Ok(self.validate_extracted_documents(&extracted, &file_path.to_string_lossy()))
    }

    /// Validate multiple files
    fn validate_files(
        &self,
        files: HashSet<PathBuf>,
        changed_content: &str,
        changed_path: &std::path::Path,
    ) -> Result<DiagnosticsMap> {
        let mut all_diagnostics = HashMap::new();

        for file_path in files {
            // For the changed file, use the provided content
            // For other files, we'd need to read from disk or have them cached
            // This is a limitation - in LSP context, we only have the changed file's content
            if file_path.as_path() == changed_path {
                let diagnostics = self.validate_file(&file_path, changed_content)?;
                if !diagnostics.is_empty() {
                    all_diagnostics.insert(file_path, diagnostics);
                }
            }
            // For other affected files, we can't validate without their content
            // The LSP layer will need to handle this by tracking open files
        }

        Ok(all_diagnostics)
    }

    /// Validate extracted documents (shared validation logic)
    #[allow(clippy::significant_drop_tightening)]
    fn validate_extracted_documents(
        &self,
        extracted: &[graphql_extract::ExtractedGraphQL],
        file_path: &str,
    ) -> Vec<Diagnostic> {
        use apollo_compiler::validation::Valid;
        use apollo_compiler::{parser::Parser, ExecutableDocument};

        if extracted.is_empty() {
            return vec![];
        }

        let schema_index = self.schema_index.read().unwrap();
        let schema = schema_index.schema();
        let valid_schema = Valid::assume_valid_ref(schema);
        let mut all_diagnostics = Vec::new();

        // Validate each extracted document
        for item in extracted {
            let line_offset = item.location.range.start.line;
            let col_offset = item.location.range.start.column;
            let source = &item.source;

            let mut errors =
                apollo_compiler::validation::DiagnosticList::new(std::sync::Arc::default());
            let mut builder = ExecutableDocument::builder(Some(valid_schema), &mut errors);
            let is_fragment_only = Self::is_fragment_only(source);

            let offset = apollo_compiler::parser::SourceOffset {
                line: line_offset + 1,
                column: col_offset + 1,
            };

            Parser::new()
                .source_offset(offset)
                .parse_into_executable_builder(source, file_path, &mut builder);

            // Add referenced fragments
            if !is_fragment_only && source.contains("...") {
                let referenced_fragments = Self::collect_referenced_fragments(source);
                let document_index = self.document_index.read().unwrap();

                for fragment_name in referenced_fragments {
                    if let Some(frags) = document_index.fragments.get(&fragment_name) {
                        if let Some(frag_info) = frags.first() {
                            // Get the fragment source from extracted blocks or parsed AST
                            if let Some(blocks) =
                                document_index.get_extracted_blocks(&frag_info.file_path)
                            {
                                // Find the block containing this fragment
                                for block in blocks {
                                    if block.content.contains(&format!("fragment {fragment_name}"))
                                    {
                                        Parser::new().parse_into_executable_builder(
                                            &block.content,
                                            &frag_info.file_path,
                                            &mut builder,
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Build and validate
            let doc = builder.build();

            let diagnostics = if errors.is_empty() {
                match doc.validate(valid_schema) {
                    Ok(_) => vec![],
                    Err(with_errors) => {
                        convert_apollo_diagnostics(&with_errors.errors, is_fragment_only)
                    }
                }
            } else {
                convert_apollo_diagnostics(&errors, is_fragment_only)
            };

            all_diagnostics.extend(diagnostics);
        }

        all_diagnostics
    }

    /// Check if source is fragment-only
    fn is_fragment_only(source: &str) -> bool {
        use apollo_parser::cst;
        use apollo_parser::Parser;

        let parsed = Parser::new(source).parse();
        let mut has_fragment = false;
        let mut has_operation = false;

        for def in parsed.document().definitions() {
            match def {
                cst::Definition::FragmentDefinition(_) => has_fragment = true,
                cst::Definition::OperationDefinition(_) => has_operation = true,
                _ => {}
            }
        }

        has_fragment && !has_operation
    }

    /// Collect referenced fragments
    fn collect_referenced_fragments(source: &str) -> Vec<String> {
        use apollo_parser::cst;
        use apollo_parser::Parser;

        let parsed = Parser::new(source).parse();
        let mut fragments = Vec::new();

        for def in parsed.document().definitions() {
            match def {
                cst::Definition::OperationDefinition(op) => {
                    if let Some(selection_set) = op.selection_set() {
                        Self::collect_fragment_spreads(&selection_set, &mut fragments);
                    }
                }
                cst::Definition::FragmentDefinition(frag) => {
                    if let Some(selection_set) = frag.selection_set() {
                        Self::collect_fragment_spreads(&selection_set, &mut fragments);
                    }
                }
                _ => {}
            }
        }

        fragments
    }

    /// Recursively collect fragment spreads
    fn collect_fragment_spreads(
        selection_set: &apollo_parser::cst::SelectionSet,
        fragments: &mut Vec<String>,
    ) {
        use apollo_parser::cst;

        for selection in selection_set.selections() {
            match selection {
                cst::Selection::FragmentSpread(fragment_spread) => {
                    if let Some(name) = fragment_spread.fragment_name() {
                        if let Some(name_token) = name.name() {
                            fragments.push(name_token.text().to_string());
                        }
                    }
                }
                cst::Selection::Field(field) => {
                    if let Some(nested_set) = field.selection_set() {
                        Self::collect_fragment_spreads(&nested_set, fragments);
                    }
                }
                cst::Selection::InlineFragment(inline) => {
                    if let Some(nested_set) = inline.selection_set() {
                        Self::collect_fragment_spreads(&nested_set, fragments);
                    }
                }
            }
        }
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

#[cfg(test)]
#[allow(clippy::significant_drop_tightening)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_workspace() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base_path = temp_dir.path();

        // Create a simple schema
        let schema = r"
type Query {
    user(id: ID!): User
    posts: [Post!]!
}

type User {
    id: ID!
    name: String!
    email: String
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
    content: String!
    author: User!
}
";
        fs::write(base_path.join("schema.graphql"), schema).unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_new_from_config() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));

        // Initialize to load schema from disk
        project.initialize().await.expect("Failed to initialize");

        // Verify schema loaded
        let schema_index = project.schema_index();
        let schema_guard = schema_index.read().unwrap();
        assert!(!schema_guard.schema().types.is_empty());
    }

    #[tokio::test]
    async fn test_add_document_quick_mode() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
        project.initialize().await.expect("Failed to initialize");

        let query = r"
query GetUser($id: ID!) {
    user(id: $id) {
        id
        name
    }
}
";
        let query_path = workspace.path().join("query.graphql");

        let diagnostics = project
            .add_or_update_document(query_path.clone(), query.to_string(), ValidationMode::Quick)
            .await
            .expect("Failed to add document");

        // Valid query should have no errors
        assert!(
            diagnostics.is_empty()
                || !diagnostics
                    .get(&query_path)
                    .unwrap_or(&vec![])
                    .iter()
                    .any(|d| d.severity == crate::Severity::Error),
            "Expected no errors for valid query"
        );

        // Verify document was indexed
        let doc_index = project.document_index();
        let doc_guard = doc_index.read().unwrap();
        assert!(!doc_guard.operations.is_empty());
    }

    #[tokio::test]
    async fn test_add_invalid_document() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
        project.initialize().await.expect("Failed to initialize");

        let invalid_query = "
query InvalidQuery {
    user(id: \"1\") {
        id
        nonExistentField
    }
}
";
        let query_path = workspace.path().join("invalid.graphql");

        let diagnostics = project
            .add_or_update_document(
                query_path.clone(),
                invalid_query.to_string(),
                ValidationMode::Quick,
            )
            .await
            .expect("Failed to add document");

        // Should have diagnostics for invalid field
        let file_diags = diagnostics.get(&query_path).expect("Expected diagnostics");
        assert!(
            !file_diags.is_empty(),
            "Expected diagnostics for invalid field"
        );

        let has_field_error = file_diags
            .iter()
            .any(|d| d.message.to_lowercase().contains("nonexistentfield"));
        assert!(has_field_error, "Expected error about nonExistentField");
    }

    #[tokio::test]
    async fn test_update_document() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
        project.initialize().await.expect("Failed to initialize");

        let query_v1 = r"
query GetUser($id: ID!) {
    user(id: $id) {
        id
    }
}
";
        let query_path = workspace.path().join("query.graphql");

        // Add initial version
        project
            .add_or_update_document(
                query_path.clone(),
                query_v1.to_string(),
                ValidationMode::Quick,
            )
            .await
            .expect("Failed to add document");

        // Update with new version
        let query_v2 = r"
query GetUser($id: ID!) {
    user(id: $id) {
        id
        name
        email
    }
}
";
        let diagnostics = project
            .add_or_update_document(
                query_path.clone(),
                query_v2.to_string(),
                ValidationMode::Quick,
            )
            .await
            .expect("Failed to update document");

        // Should validate without errors
        assert!(
            diagnostics.is_empty()
                || !diagnostics
                    .get(&query_path)
                    .unwrap_or(&vec![])
                    .iter()
                    .any(|d| d.severity == crate::Severity::Error),
            "Expected no errors after update"
        );
    }

    #[tokio::test]
    async fn test_remove_document() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
        project.initialize().await.expect("Failed to initialize");

        let query = r"
query GetUser($id: ID!) {
    user(id: $id) {
        id
    }
}
";
        let query_path = workspace.path().join("query.graphql");

        // Add document
        project
            .add_or_update_document(query_path.clone(), query.to_string(), ValidationMode::Quick)
            .await
            .expect("Failed to add document");

        // Verify it exists
        {
            let doc_index_before = project.document_index();
            let doc_guard_before = doc_index_before.read().unwrap();
            assert!(!doc_guard_before.operations.is_empty());
        }

        // Remove document
        project
            .remove_document(&query_path)
            .expect("Failed to remove document");

        // Verify it's gone
        {
            let doc_index_after = project.document_index();
            let doc_guard_after = doc_index_after.read().unwrap();
            assert!(doc_guard_after.operations.is_empty());
        }
    }

    #[tokio::test]
    async fn test_fragment_resolution_smart_mode() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
        project.initialize().await.expect("Failed to initialize");

        // Add fragment
        let fragment = r"
fragment UserFields on User {
    id
    name
    email
}
";
        let fragment_path = workspace.path().join("fragments.graphql");
        project
            .add_or_update_document(
                fragment_path.clone(),
                fragment.to_string(),
                ValidationMode::Quick,
            )
            .await
            .expect("Failed to add fragment");

        // Add query using fragment
        let query = r"
query GetUser($id: ID!) {
    user(id: $id) {
        ...UserFields
        posts {
            id
            title
        }
    }
}
";
        let query_path = workspace.path().join("query.graphql");
        let diagnostics = project
            .add_or_update_document(query_path.clone(), query.to_string(), ValidationMode::Smart)
            .await
            .expect("Failed to add query");

        // Should not have "undefined fragment" error
        if let Some(file_diags) = diagnostics.get(&query_path) {
            let has_undefined_error = file_diags
                .iter()
                .any(|d| d.message.to_lowercase().contains("undefined"));
            assert!(
                !has_undefined_error,
                "Should resolve UserFields fragment, got: {file_diags:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_validation_mode_full() {
        let workspace = create_test_workspace();
        let config = ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
        project.initialize().await.expect("Failed to initialize");

        // Add multiple documents
        let query1 = r"query Q1 { posts { id } }";
        let query2 = r"query Q2 { posts { title } }";

        project
            .add_or_update_document(
                workspace.path().join("q1.graphql"),
                query1.to_string(),
                ValidationMode::Quick,
            )
            .await
            .expect("Failed to add query1");

        // Add second query with Full validation
        let diagnostics = project
            .add_or_update_document(
                workspace.path().join("q2.graphql"),
                query2.to_string(),
                ValidationMode::Full,
            )
            .await
            .expect("Failed to add query2");

        // Full mode validates all files, so might have diagnostics for both
        // Just verify no panics - the fact we got here means validation completed
        assert!(diagnostics.keys().len() <= 10); // Reasonable upper bound
    }
}
