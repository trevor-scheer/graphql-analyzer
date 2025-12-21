use crate::{DocumentIndex, FragmentInfo, OperationInfo, OperationType, ProjectError, Result};
use apollo_parser::{cst, Parser};
use graphql_config::DocumentsConfig;
use graphql_extract::{extract_from_file, ExtractConfig};
use std::path::{Path, PathBuf};

/// Document loader for loading GraphQL operations and fragments from various sources
pub struct DocumentLoader {
    config: DocumentsConfig,
    base_path: Option<PathBuf>,
    extract_config: ExtractConfig,
}

impl DocumentLoader {
    #[must_use]
    pub fn new(config: DocumentsConfig) -> Self {
        Self {
            config,
            base_path: None,
            extract_config: ExtractConfig::default(),
        }
    }

    #[must_use]
    pub fn with_base_path(mut self, path: impl AsRef<Path>) -> Self {
        self.base_path = Some(path.as_ref().to_path_buf());
        self
    }

    #[must_use]
    pub fn with_extract_config(mut self, config: ExtractConfig) -> Self {
        self.extract_config = config;
        self
    }

    /// Load all documents and build an index
    #[tracing::instrument(skip(self), fields(pattern_count = self.config.patterns().len()))]
    pub fn load(&self) -> Result<DocumentIndex> {
        use std::time::Instant;
        let start = Instant::now();

        let patterns: Vec<_> = self.config.patterns().into_iter().collect();
        tracing::info!("Starting document loading");

        let mut index = DocumentIndex::new();
        let mut total_files_matched = 0;
        let mut total_files_loaded = 0;
        let mut total_files_failed = 0;

        // If we have a base path, process all patterns together (supports negation)
        // Otherwise, process patterns one by one (backward compatibility for tests)
        if self.base_path.is_some() {
            let paths = self.find_all_files(&patterns)?;
            total_files_matched = paths.len();
            tracing::debug!(files_found = paths.len(), "Files matched patterns");

            let _load_files_span =
                tracing::debug_span!("load_files", file_count = paths.len()).entered();
            for path in paths {
                if let Err(e) = self.load_file(&path, &mut index) {
                    // Log error but continue with other files
                    eprintln!("Warning: Failed to load {}: {}", path.display(), e);
                    total_files_failed += 1;
                } else {
                    total_files_loaded += 1;
                }
            }
        } else {
            // Fallback for tests and cases without base path
            for pattern in &patterns {
                let _pattern_span =
                    tracing::debug_span!("find_files_for_pattern", pattern = %pattern).entered();
                let paths = self.find_files(pattern)?;
                total_files_matched += paths.len();
                tracing::debug!(files_found = paths.len(), "Files matched pattern");

                for path in paths {
                    if let Err(e) = self.load_file(&path, &mut index) {
                        // Log error but continue with other files
                        eprintln!("Warning: Failed to load {}: {}", path.display(), e);
                        total_files_failed += 1;
                    } else {
                        total_files_loaded += 1;
                    }
                }
            }
        }

        let duration = start.elapsed();
        tracing::info!(
            total_files_matched,
            total_files_loaded,
            total_files_failed,
            operations = index.operations.len(),
            fragments = index.fragments.len(),
            duration_ms = duration.as_millis(),
            "Document loading completed"
        );

        Ok(index)
    }

    /// Find files matching a glob pattern (used when no base path is set)
    #[tracing::instrument(skip(self), fields(pattern = %pattern))]
    fn find_files(&self, pattern: &str) -> Result<Vec<PathBuf>> {
        // Expand brace patterns like {ts,tsx} since glob crate doesn't support them
        let expanded_patterns = Self::expand_braces(pattern);
        tracing::debug!(
            expanded_count = expanded_patterns.len(),
            "Brace patterns expanded"
        );

        let mut files = Vec::new();
        let mut seen_normalized: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for expanded_pattern in expanded_patterns {
            let full_pattern = self.base_path.as_ref().map_or_else(
                || expanded_pattern.clone(),
                |base| base.join(&expanded_pattern).display().to_string(),
            );

            let _glob_span = tracing::trace_span!("glob_match", pattern = %full_pattern).entered();
            for entry in glob::glob(&full_pattern)
                .map_err(|e| ProjectError::DocumentLoad(format!("Invalid glob pattern: {e}")))?
            {
                match entry {
                    Ok(path) if path.is_file() => {
                        // Skip files in node_modules directories
                        if path.components().any(|c| c.as_os_str() == "node_modules") {
                            continue;
                        }

                        // Normalize path to detect duplicates with different formats
                        // (e.g., "./src/file.graphql" vs "src/file.graphql")
                        let normalized = Self::normalize_path(&path);
                        if seen_normalized.insert(normalized) {
                            files.push(path);
                        }
                    }
                    Ok(_) => {} // Skip directories
                    Err(e) => {
                        return Err(ProjectError::DocumentLoad(format!("Glob error: {e}")));
                    }
                }
            }
        }

        tracing::debug!(files_found = files.len(), "Files found after glob matching");
        Ok(files)
    }

    /// Find files matching all patterns (supports gitignore-style negation)
    fn find_all_files(&self, patterns: &[&str]) -> Result<Vec<PathBuf>> {
        // Check if any patterns are negations (start with !)
        let has_negations = patterns.iter().any(|p| p.trim().starts_with('!'));

        if !has_negations {
            // Fast path: use glob for non-negation patterns
            let expanded: Vec<String> = patterns
                .iter()
                .flat_map(|p| Self::expand_braces(p))
                .collect();
            let base = self.base_path.as_ref().ok_or_else(|| {
                ProjectError::DocumentLoad("Base path required for pattern matching".to_string())
            })?;
            return Self::find_files_with_glob(&expanded, base);
        }

        // Gitignore-style matching with negation support
        let expanded: Vec<String> = patterns
            .iter()
            .flat_map(|p| Self::expand_braces(p))
            .collect();
        let base = self.base_path.as_ref().ok_or_else(|| {
            ProjectError::DocumentLoad("Base path required for pattern matching".to_string())
        })?;
        Self::find_files_with_gitignore_style(&expanded, base)
    }

    /// Fast path for finding files without negation patterns
    fn find_files_with_glob(patterns: &[String], base_path: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut seen_normalized: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for pattern in patterns {
            let full_pattern = base_path.join(pattern).display().to_string();

            for entry in glob::glob(&full_pattern)
                .map_err(|e| ProjectError::DocumentLoad(format!("Invalid glob pattern: {e}")))?
            {
                match entry {
                    Ok(path) if path.is_file() => {
                        // Skip files in node_modules directories
                        if path.components().any(|c| c.as_os_str() == "node_modules") {
                            continue;
                        }

                        // Normalize path to detect duplicates with different formats
                        // (e.g., "./src/file.graphql" vs "src/file.graphql")
                        let normalized = Self::normalize_path(&path);
                        if seen_normalized.insert(normalized) {
                            files.push(path);
                        }
                    }
                    Ok(_) => {} // Skip directories
                    Err(e) => {
                        return Err(ProjectError::DocumentLoad(format!("Glob error: {e}")));
                    }
                }
            }
        }

        Ok(files)
    }

    /// Find files using gitignore-style pattern matching (supports negation)
    fn find_files_with_gitignore_style(
        patterns: &[String],
        base_path: &Path,
    ) -> Result<Vec<PathBuf>> {
        use ignore::gitignore::GitignoreBuilder;

        // Separate positive and negation patterns
        let (positive_patterns, negation_patterns): (Vec<_>, Vec<_>) =
            patterns.iter().partition(|p| !p.starts_with('!'));

        // Convert Vec<&String> to Vec<String> for glob matching
        let positive_owned: Vec<String> = positive_patterns.iter().map(|s| (*s).clone()).collect();

        // First, use glob to match positive patterns
        let mut files = Self::find_files_with_glob(&positive_owned, base_path)?;

        if negation_patterns.is_empty() {
            return Ok(files);
        }

        // Build gitignore matcher for negations
        let mut builder = GitignoreBuilder::new(base_path);

        // Add all patterns to the builder
        for pattern in patterns {
            builder.add_line(None, pattern).map_err(|e| {
                ProjectError::DocumentLoad(format!("Invalid gitignore pattern: {e}"))
            })?;
        }

        let gitignore = builder.build().map_err(|e| {
            ProjectError::DocumentLoad(format!("Failed to build gitignore matcher: {e}"))
        })?;

        // Filter files using gitignore matcher
        files.retain(|path| {
            // Get relative path from base for matching
            let relative_path = path.strip_prefix(base_path).unwrap_or(path);

            // Check if file should be excluded
            !gitignore.matched(relative_path, false).is_ignore()
        });

        Ok(files)
    }

    /// Expand brace patterns like {ts,tsx} into multiple patterns
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

    /// Normalize a file path by removing "./" components
    ///
    /// This ensures that files matched by different glob patterns
    /// (e.g., "./src/file.graphql" vs "src/file.graphql") are treated
    /// as the same file and not indexed twice.
    ///
    /// For example:
    /// - "./src/file.graphql" -> "src/file.graphql"
    /// - "/tmp/./src/file.graphql" -> "/tmp/src/file.graphql"
    fn normalize_path(path: &Path) -> String {
        // Normalize path components to remove "." references
        let components: Vec<_> = path
            .components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .collect();

        // Reconstruct path from components
        let normalized = components.iter().collect::<PathBuf>();
        normalized.display().to_string()
    }

    /// Load a single file and add operations/fragments to the index
    #[tracing::instrument(skip(self, index), fields(file = %path.display()), level = "debug")]
    fn load_file(&self, path: &Path, index: &mut DocumentIndex) -> Result<()> {
        use std::fs;

        // Read the file content
        let content = fs::read_to_string(path)
            .map_err(|e| ProjectError::DocumentLoad(format!("Failed to read file: {e}")))?;

        // Parse the full content once and cache it
        let parsed = Parser::new(&content).parse();
        let parsed_arc = std::sync::Arc::new(parsed);

        // Extract GraphQL from the file
        let extracted = extract_from_file(path, &self.extract_config)
            .map_err(|e| ProjectError::DocumentLoad(format!("Extract error: {e}")))?;

        let file_path = Self::normalize_path(path);

        // Cache the parsed AST for pure GraphQL files
        index.cache_ast(file_path.clone(), parsed_arc);

        // Cache extracted blocks with their parsed ASTs (for TypeScript/JavaScript files)
        if !extracted.is_empty() {
            let mut cached_blocks = Vec::new();
            for item in &extracted {
                // Parse each extracted block and cache it
                let block_parsed = Parser::new(&item.source).parse();
                let block = crate::ExtractedBlock {
                    content: item.source.clone(),
                    offset: item.location.offset,
                    length: item.location.length,
                    start_line: item.location.range.start.line,
                    start_column: item.location.range.start.column,
                    end_line: item.location.range.end.line,
                    end_column: item.location.range.end.column,
                    parsed: std::sync::Arc::new(block_parsed),
                };
                cached_blocks.push(block);
            }
            index.cache_extracted_blocks(file_path.clone(), cached_blocks);
        }

        // Parse each extracted GraphQL document and add to index
        for item in extracted {
            Self::parse_and_index(&item, &file_path, index);
        }

        Ok(())
    }

    /// Parse GraphQL source and add operations/fragments to index
    pub fn parse_and_index(
        item: &graphql_extract::ExtractedGraphQL,
        file_path: &str,
        index: &mut DocumentIndex,
    ) {
        use apollo_parser::cst::CstNode;

        let source = &item.source;
        // Get the starting position in the original file for this extracted block
        let base_line = item.location.range.start.line;
        let base_column = item.location.range.start.column;

        let parser = Parser::new(source);
        let tree = parser.parse();

        // Skip if there are syntax errors
        if tree.errors().len() > 0 {
            return; // Silently skip invalid documents
        }

        let document = tree.document();

        for definition in document.definitions() {
            match definition {
                cst::Definition::OperationDefinition(op) => {
                    let operation_type = match op.operation_type() {
                        Some(op_type) if op_type.query_token().is_some() => OperationType::Query,
                        Some(op_type) if op_type.mutation_token().is_some() => {
                            OperationType::Mutation
                        }
                        Some(op_type) if op_type.subscription_token().is_some() => {
                            OperationType::Subscription
                        }
                        _ => OperationType::Query, // Default to query
                    };

                    let (name, line, column) = op.name().map_or((None, 0, 0), |name_node| {
                        let name_str = name_node.text().to_string();
                        let syntax_node = name_node.syntax();
                        let offset: usize = syntax_node.text_range().start().into();
                        let (rel_line, rel_col) = Self::offset_to_line_col(source, offset);

                        // Add the base position from the extracted block
                        let abs_line = base_line + rel_line;
                        let abs_col = if rel_line == 0 {
                            base_column + rel_col
                        } else {
                            rel_col
                        };

                        (Some(name_str), abs_line, abs_col)
                    });

                    let info = OperationInfo {
                        name: name.clone(),
                        operation_type,
                        file_path: file_path.to_string(),
                        line,
                        column,
                    };

                    index.add_operation(name, info);
                }
                cst::Definition::FragmentDefinition(frag) => {
                    if let (Some(name_node), Some(type_cond)) =
                        (frag.fragment_name(), frag.type_condition())
                    {
                        let name = name_node
                            .name()
                            .map_or_else(String::new, |n| n.text().to_string());
                        let type_condition = type_cond
                            .named_type()
                            .and_then(|nt| nt.name())
                            .map_or_else(String::new, |n| n.text().to_string());

                        // Get position of fragment name
                        let (line, column) = name_node.name().map_or((0, 0), |name_token| {
                            let syntax_node = name_token.syntax();
                            let offset: usize = syntax_node.text_range().start().into();
                            let (rel_line, rel_col) = Self::offset_to_line_col(source, offset);

                            // Add the base position from the extracted block
                            let abs_line = base_line + rel_line;
                            let abs_col = if rel_line == 0 {
                                base_column + rel_col
                            } else {
                                rel_col
                            };

                            (abs_line, abs_col)
                        });

                        let info = FragmentInfo {
                            name: name.clone(),
                            type_condition,
                            file_path: file_path.to_string(),
                            line,
                            column,
                        };

                        index.add_fragment(name, info);
                    }
                }
                _ => {} // Skip schema definitions in document files
            }
        }
    }

    /// Convert a byte offset to a line and column (0-indexed)
    fn offset_to_line_col(document: &str, offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        let mut current_offset = 0;

        for ch in document.chars() {
            if current_offset >= offset {
                break;
            }

            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }

            current_offset += ch.len_utf8();
        }

        (line, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_config::DocumentsConfig;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_load_graphql_files() {
        let temp_dir = tempdir().unwrap();

        // Create test GraphQL files
        let query_file = temp_dir.path().join("queries.graphql");
        fs::write(
            &query_file,
            r"
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                }
            }

            mutation UpdateUser($id: ID!, $name: String!) {
                updateUser(id: $id, name: $name) {
                    id
                    name
                }
            }
        ",
        )
        .unwrap();

        let fragment_file = temp_dir.path().join("fragments.graphql");
        fs::write(
            &fragment_file,
            r"
            fragment UserFields on User {
                id
                name
                email
            }
        ",
        )
        .unwrap();

        let pattern = temp_dir.path().join("*.graphql").display().to_string();
        let config = DocumentsConfig::Pattern(pattern);
        let loader = DocumentLoader::new(config);
        let index = loader.load().unwrap();

        // Check operation
        let get_user = index.get_operation("GetUser");
        assert!(get_user.is_some());
        assert_eq!(get_user.unwrap().operation_type, OperationType::Query);

        let update_user = index.get_operation("UpdateUser");
        assert!(update_user.is_some());
        assert_eq!(update_user.unwrap().operation_type, OperationType::Mutation);

        // Check fragment
        let user_fields = index.get_fragment("UserFields");
        assert!(user_fields.is_some());
        assert_eq!(user_fields.unwrap().type_condition, "User");
    }

    #[test]
    fn test_load_typescript_files() {
        let temp_dir = tempdir().unwrap();

        // Create TypeScript file with embedded GraphQL
        let ts_file = temp_dir.path().join("queries.ts");
        fs::write(
            &ts_file,
            r"
            import { gql } from '@apollo/client';

            export const GET_USER = gql`
                query GetUser($id: ID!) {
                    user(id: $id) {
                        id
                        name
                    }
                }
            `;

            export const USER_FRAGMENT = gql`
                fragment UserInfo on User {
                    id
                    name
                    email
                }
            `;
        ",
        )
        .unwrap();

        let pattern = temp_dir.path().join("*.ts").display().to_string();
        let config = DocumentsConfig::Pattern(pattern);
        let loader = DocumentLoader::new(config);
        let index = loader.load().unwrap();

        // Check operation
        let get_user = index.get_operation("GetUser");
        assert!(get_user.is_some());

        // Check fragment
        let user_info = index.get_fragment("UserInfo");
        assert!(user_info.is_some());
    }

    #[test]
    fn test_load_multiple_patterns() {
        let temp_dir = tempdir().unwrap();

        let query_file = temp_dir.path().join("query.graphql");
        fs::write(&query_file, "query Test { __typename }").unwrap();

        let ts_file = temp_dir.path().join("query.ts");
        fs::write(
            &ts_file,
            r"
            import { gql } from '@apollo/client';
            const QUERY = gql`query TypeScript { __typename }`;
        ",
        )
        .unwrap();

        let patterns = vec![
            temp_dir.path().join("*.graphql").display().to_string(),
            temp_dir.path().join("*.ts").display().to_string(),
        ];
        let config = DocumentsConfig::Patterns(patterns);
        let loader = DocumentLoader::new(config);
        let index = loader.load().unwrap();

        assert!(index.get_operation("Test").is_some());
        assert!(index.get_operation("TypeScript").is_some());
    }

    #[test]
    fn test_skip_invalid_documents() {
        let temp_dir = tempdir().unwrap();

        // Create file with invalid GraphQL
        let invalid_file = temp_dir.path().join("invalid.graphql");
        fs::write(&invalid_file, "query { this is not valid }").unwrap();

        // Create file with valid GraphQL
        let valid_file = temp_dir.path().join("valid.graphql");
        fs::write(&valid_file, "query Valid { __typename }").unwrap();

        let pattern = temp_dir.path().join("*.graphql").display().to_string();
        let config = DocumentsConfig::Pattern(pattern);
        let loader = DocumentLoader::new(config);
        let index = loader.load().unwrap();

        // Should only have the valid query
        assert!(index.get_operation("Valid").is_some());
    }

    #[test]
    fn test_skip_node_modules() {
        let temp_dir = tempdir().unwrap();

        // Create a node_modules directory with a GraphQL file
        let node_modules = temp_dir.path().join("node_modules").join("some-package");
        fs::create_dir_all(&node_modules).unwrap();
        let node_modules_file = node_modules.join("schema.graphql");
        fs::write(&node_modules_file, "query NodeModulesQuery { __typename }").unwrap();

        // Create a regular GraphQL file outside node_modules
        let regular_file = temp_dir.path().join("query.graphql");
        fs::write(&regular_file, "query RegularQuery { __typename }").unwrap();

        let pattern = temp_dir.path().join("**/*.graphql").display().to_string();
        let config = DocumentsConfig::Pattern(pattern);
        let loader = DocumentLoader::new(config);
        let index = loader.load().unwrap();

        // Should only have the regular query, not the one from node_modules
        assert!(index.get_operation("RegularQuery").is_some());
        assert!(index.get_operation("NodeModulesQuery").is_none());
    }

    #[test]
    fn test_normalize_path() {
        // Test stripping "./" prefix
        let path = Path::new("./src/queries.graphql");
        assert_eq!(DocumentLoader::normalize_path(path), "src/queries.graphql");

        // Test path without "./" prefix remains unchanged
        let path = Path::new("src/queries.graphql");
        assert_eq!(DocumentLoader::normalize_path(path), "src/queries.graphql");

        // Test absolute paths remain unchanged
        let path = Path::new("/absolute/path/to/file.graphql");
        assert_eq!(
            DocumentLoader::normalize_path(path),
            "/absolute/path/to/file.graphql"
        );

        // Test nested "./" components are removed
        let path = Path::new("./src/./nested/file.graphql");
        assert_eq!(
            DocumentLoader::normalize_path(path),
            "src/nested/file.graphql"
        );

        // Test absolute path with embedded "./" components
        let path = Path::new("/tmp/./src/file.graphql");
        assert_eq!(
            DocumentLoader::normalize_path(path),
            "/tmp/src/file.graphql"
        );
    }

    #[test]
    fn test_no_duplicate_indexing_with_different_path_formats() {
        let temp_dir = tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        // Create a single GraphQL file with a fragment
        let fragment_file = src_dir.join("fragments.graphql");
        fs::write(
            &fragment_file,
            r"
            fragment UserFields on User {
                id
                name
            }
        ",
        )
        .unwrap();

        // Use multiple patterns that should match the same file
        // but with different path formats (with and without "./" prefix)
        let patterns = vec![
            "./src/**/*.graphql".to_string(),
            "src/**/*.graphql".to_string(),
        ];
        let config = DocumentsConfig::Patterns(patterns);
        let loader = DocumentLoader::new(config).with_base_path(temp_dir.path());
        let index = loader.load().unwrap();

        // The fragment should only be indexed once, not twice
        let fragments = index.get_fragments_by_name("UserFields");
        assert!(fragments.is_some(), "Fragment should be indexed");
        let fragments = fragments.unwrap();
        assert_eq!(
            fragments.len(),
            1,
            "Fragment should only be indexed once, not {} times. Paths: {:?}",
            fragments.len(),
            fragments.iter().map(|f| &f.file_path).collect::<Vec<_>>()
        );
    }
}
