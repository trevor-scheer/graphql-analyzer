use crate::{
    convert_apollo_diagnostics, Diagnostic, DocumentIndex, DocumentLoader, GraphQLConfig,
    ProjectConfig, Result, SchemaIndex, SchemaLoader,
};
use graphql_extract::ExtractConfig;
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
    /// File contents for validation (`file_path` -> `content`)
    /// Stored during `load()` for efficient validation without re-reading from disk
    file_contents: HashMap<PathBuf, String>,
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
    /// - Stores file contents for validation
    /// - Ready for validation
    pub async fn load(config: ProjectConfig, base_dir: Option<PathBuf>) -> Result<Self> {
        let mut file_contents = HashMap::new();

        // Load schema
        let mut schema_loader = SchemaLoader::new(config.schema.clone());
        if let Some(ref base_path) = base_dir {
            schema_loader = schema_loader.with_base_path(base_path);
        }
        let schema_files = schema_loader.load_with_paths().await?;

        // Store schema file contents
        for (path, content) in &schema_files {
            file_contents.insert(PathBuf::from(path), content.clone());
        }

        let schema_index = SchemaIndex::from_schema_files(schema_files);

        // Load documents (if configured)
        let (document_index, document_files) = if let Some(ref documents_config) = config.documents
        {
            let mut document_loader = DocumentLoader::new(documents_config.clone());
            if let Some(ref base_path) = base_dir {
                document_loader = document_loader.with_base_path(base_path);
            }

            // Use the new load_with_contents method to get both index and contents
            let (index, files) = Self::load_documents_with_contents(&document_loader)?;
            (index, files)
        } else {
            (DocumentIndex::new(), HashMap::new())
        };

        // Merge document file contents
        file_contents.extend(document_files);

        Ok(Self {
            config,
            base_dir,
            schema_index,
            document_index,
            file_contents,
        })
    }

    /// Load documents with their file contents
    ///
    /// This is a helper method that loads documents and captures their file contents
    /// for later validation.
    fn load_documents_with_contents(
        loader: &DocumentLoader,
    ) -> Result<(DocumentIndex, HashMap<PathBuf, String>)> {
        // Load the index first
        let loaded_index = loader.load()?;
        let mut file_contents = HashMap::new();

        // Get all unique file paths from the loaded index
        let mut file_paths = std::collections::HashSet::new();
        for operations in loaded_index.operations.values() {
            for op in operations {
                file_paths.insert(PathBuf::from(&op.file_path));
            }
        }
        for fragments in loaded_index.fragments.values() {
            for frag in fragments {
                file_paths.insert(PathBuf::from(&frag.file_path));
            }
        }

        // Read the contents of each file
        for file_path in file_paths {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                file_contents.insert(file_path, content);
            }
        }

        Ok((loaded_index, file_contents))
    }

    /// Validate the entire project
    ///
    /// Returns diagnostics for all files in the project.
    /// This runs:
    /// - Schema validation (TODO)
    /// - Document validation (all documents)
    ///
    /// Note: Linting should be performed by the consumer (LSP/CLI) after calling this method.
    /// The consumer can use the graphql-linter crate with the schema and document indices.
    ///
    /// No caching, no incrementality - just straightforward validation.
    #[must_use]
    pub fn validate_all(&self) -> DiagnosticsMap {
        let mut all_diagnostics = HashMap::new();

        // Validate each document file (skip schema files)
        for (file_path, content) in &self.file_contents {
            // Skip schema files - they're not executable documents
            if self.is_schema_file(file_path) {
                continue;
            }

            let diagnostics = self.validate_file(file_path, content);
            if !diagnostics.is_empty() {
                all_diagnostics.insert(file_path.clone(), diagnostics);
            }
        }

        all_diagnostics
    }

    /// Validate a single file
    fn validate_file(&self, file_path: &std::path::Path, content: &str) -> Vec<Diagnostic> {
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
        let extracted = match extract_from_source(content, language, &extract_config) {
            Ok(extracted) => extracted,
            Err(e) => {
                // If extraction fails, return an error diagnostic
                return vec![Diagnostic::error(
                    crate::Range {
                        start: crate::Position {
                            line: 0,
                            character: 0,
                        },
                        end: crate::Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    format!("Failed to extract GraphQL: {e}"),
                )
                .with_source("graphql-extract")];
            }
        };

        // Validate extracted documents
        self.validate_extracted_documents(&extracted, &file_path.to_string_lossy())
    }

    /// Get extract configuration for this project
    fn get_extract_config(&self) -> ExtractConfig {
        self.config
            .extensions
            .as_ref()
            .and_then(|ext| ext.get("extractConfig"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    /// Validate extracted GraphQL documents from a file
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

        let schema = self.schema_index.schema();
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

            // Use source_offset for accurate error reporting
            let offset = apollo_compiler::parser::SourceOffset {
                line: line_offset + 1, // Convert to 1-indexed
                column: col_offset + 1,
            };

            Parser::new()
                .source_offset(offset)
                .parse_into_executable_builder(source, file_path, &mut builder);

            // Add referenced fragments if this document uses fragment spreads
            if !is_fragment_only && source.contains("...") {
                let referenced_fragments = Self::collect_referenced_fragments(source);
                for fragment_name in referenced_fragments {
                    if let Some(frag_info) = self.get_fragment(&fragment_name) {
                        if let Some(fragment_source) =
                            self.extract_fragment_source(&frag_info.file_path, &fragment_name)
                        {
                            Parser::new().parse_into_executable_builder(
                                &fragment_source,
                                &frag_info.file_path,
                                &mut builder,
                            );
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

    /// Check if source contains only fragment definitions
    fn is_fragment_only(source: &str) -> bool {
        use apollo_parser::cst;
        use apollo_parser::Parser;

        let parsed = Parser::new(source).parse();

        let mut has_fragment = false;
        let mut has_operation = false;

        for def in parsed.document().definitions() {
            match def {
                cst::Definition::FragmentDefinition(_) => {
                    has_fragment = true;
                }
                cst::Definition::OperationDefinition(_) => {
                    has_operation = true;
                }
                _ => {}
            }
        }

        has_fragment && !has_operation
    }

    /// Collect fragment names referenced in source (recursively)
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

    /// Recursively collect fragment spread names from a selection set
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

    /// Get fragment info by name
    fn get_fragment(&self, name: &str) -> Option<&crate::FragmentInfo> {
        self.document_index
            .fragments
            .get(name)
            .and_then(|frags| frags.first())
    }

    /// Extract a specific fragment's source from a file
    fn extract_fragment_source(&self, file_path: &str, fragment_name: &str) -> Option<String> {
        use apollo_parser::cst;
        use apollo_parser::cst::CstNode;
        use apollo_parser::Parser;

        let content = self.file_contents.get(&PathBuf::from(file_path))?;
        let parsed = Parser::new(content).parse();

        for def in parsed.document().definitions() {
            if let cst::Definition::FragmentDefinition(frag) = def {
                if let Some(name) = frag.fragment_name() {
                    if let Some(name_token) = name.name() {
                        if name_token.text() == fragment_name {
                            return Some(frag.syntax().text().to_string());
                        }
                    }
                }
            }
        }

        None
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

    /// Check if a file path matches any schema pattern
    #[must_use]
    pub fn is_schema_file(&self, file_path: &std::path::Path) -> bool {
        use glob::Pattern;

        let schema_patterns = self.config.schema.paths();

        // Get the file path as a string for matching
        let Some(file_str) = file_path.to_str() else {
            return false;
        };

        // Check if file matches any schema pattern
        for pattern_str in schema_patterns {
            // Resolve the pattern to an absolute path if we have a base_dir
            if let Some(ref base) = self.base_dir {
                // Normalize the pattern by stripping leading ./ if present
                let normalized_pattern = pattern_str.strip_prefix("./").unwrap_or(pattern_str);

                // Join with base directory to get absolute path
                let full_path = base.join(normalized_pattern);

                // Canonicalize both paths if possible for comparison
                let file_canonical = file_path.canonicalize().ok();
                let pattern_canonical = full_path.canonicalize().ok();

                if let (Some(file_canon), Some(pattern_canon)) = (file_canonical, pattern_canonical)
                {
                    if file_canon == pattern_canon {
                        return true;
                    }
                }

                // Also try glob pattern matching against the file path
                if let Ok(pattern) = Pattern::new(full_path.to_str().unwrap_or("")) {
                    if pattern.matches(file_str) {
                        return true;
                    }
                }
            }

            // Try pattern matching against the file path directly (relative paths)
            if let Ok(pattern) = Pattern::new(pattern_str) {
                if pattern.matches(file_str) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
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

        // Create a fragment
        let fragment_user = r"
fragment UserFields on User {
    id
    name
    email
}
";
        fs::write(base_path.join("fragments.graphql"), fragment_user).unwrap();

        // Create a valid query
        let valid_query = r"
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
        fs::write(base_path.join("query.graphql"), valid_query).unwrap();

        // Create an invalid query
        let invalid_query = "
query InvalidQuery {
    user(id: \"1\") {
        id
        nonExistentField
    }
}
";
        fs::write(base_path.join("invalid.graphql"), invalid_query).unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_loads_from_disk() {
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

        let project = StaticGraphQLProject::load(config, Some(workspace.path().to_path_buf()))
            .await
            .expect("Failed to load project");

        // Verify schema loaded
        assert!(!project.schema_index().schema().types.is_empty());

        // Verify documents loaded
        assert!(!project.document_index().operations.is_empty());
        assert!(!project.document_index().fragments.is_empty());
    }

    #[tokio::test]
    async fn test_validates_all_files() {
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

        let project = StaticGraphQLProject::load(config, Some(workspace.path().to_path_buf()))
            .await
            .expect("Failed to load project");

        let diagnostics = project.validate_all();

        // Should have diagnostics for invalid file
        let has_invalid_diags = diagnostics
            .iter()
            .any(|(path, diags)| path.to_string_lossy().contains("invalid") && !diags.is_empty());

        assert!(
            has_invalid_diags,
            "Expected diagnostics for invalid.graphql"
        );
    }

    #[tokio::test]
    async fn test_resolves_fragments() {
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

        let project = StaticGraphQLProject::load(config, Some(workspace.path().to_path_buf()))
            .await
            .expect("Failed to load project");

        let diagnostics = project.validate_all();

        // query.graphql uses UserFields fragment - should not have "undefined" error
        let query_diags = diagnostics
            .iter()
            .find(|(path, _)| path.to_string_lossy().contains("query.graphql"));

        if let Some((_, diags)) = query_diags {
            let has_undefined_error = diags
                .iter()
                .any(|d| d.message.to_lowercase().contains("undefined"));

            assert!(
                !has_undefined_error,
                "Should resolve UserFields fragment, but got: {diags:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_reports_validation_errors() {
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

        let project = StaticGraphQLProject::load(config, Some(workspace.path().to_path_buf()))
            .await
            .expect("Failed to load project");

        let diagnostics = project.validate_all();

        // invalid.graphql should have error about nonExistentField
        let invalid_diags = diagnostics
            .iter()
            .find(|(path, _)| path.to_string_lossy().contains("invalid.graphql"));

        assert!(
            invalid_diags.is_some(),
            "Expected diagnostics for invalid.graphql"
        );

        let (_, diags) = invalid_diags.unwrap();
        let has_field_error = diags
            .iter()
            .any(|d| d.message.to_lowercase().contains("nonexistentfield"));

        assert!(
            has_field_error,
            "Expected error about nonExistentField, got: {diags:?}"
        );
    }

    #[tokio::test]
    async fn test_from_config_with_base() {
        let workspace = create_test_workspace();

        // Create .graphqlrc.yaml
        let graphql_config = "
schema: schema.graphql
documents: \"*.graphql\"
";
        fs::write(workspace.path().join(".graphqlrc.yaml"), graphql_config).unwrap();

        let config_path = graphql_config::find_config(workspace.path())
            .expect("Failed to find config")
            .expect("No config found");

        let config = graphql_config::load_config(&config_path).expect("Failed to load config");

        let projects = StaticGraphQLProject::from_config_with_base(&config, workspace.path())
            .await
            .expect("Failed to create projects");

        assert!(!projects.is_empty(), "Expected at least one project");

        let (_name, project) = &projects[0];
        let diagnostics = project.validate_all();

        // Should validate files
        assert!(!diagnostics.is_empty(), "Expected some validation results");
    }
}
