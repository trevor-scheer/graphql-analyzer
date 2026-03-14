use graphql_base_db::{DocumentKind, Language};

use crate::helpers::path_to_file_uri;
use crate::types::FilePath;

/// Information about a loaded file from document discovery
#[derive(Debug, Clone)]
pub struct LoadedFile {
    /// The file path (as a URI string)
    pub path: FilePath,
    /// The source language
    pub language: Language,
    /// The document kind
    pub document_kind: DocumentKind,
}

/// File data that has been read from disk but not yet registered.
/// Used to separate file I/O from lock acquisition.
#[derive(Debug)]
pub struct DiscoveredFile {
    /// The file path (as a URI string)
    pub path: FilePath,
    /// The file content
    pub content: String,
    /// The source language
    pub language: Language,
    /// The document kind
    pub document_kind: DocumentKind,
}

/// A content mismatch error found during file discovery.
///
/// This indicates a file's content doesn't match its expected `DocumentKind`
/// based on which config pattern matched it.
#[derive(Debug, Clone)]
pub struct ContentMismatchError {
    /// The pattern that matched this file
    pub pattern: String,
    /// Path to the file with mismatched content
    pub file_path: std::path::PathBuf,
    /// What kind was expected (based on config)
    pub expected: graphql_config::FileType,
    /// Names of definitions that don't belong
    pub unexpected_definitions: Vec<String>,
}

/// Result of file discovery, containing both files and any validation errors.
#[derive(Debug, Default)]
pub struct FileDiscoveryResult {
    /// Successfully discovered files
    pub files: Vec<DiscoveredFile>,
    /// Content mismatch errors found during discovery
    pub errors: Vec<ContentMismatchError>,
    /// Patterns that matched no files on disk
    pub unmatched_patterns: Vec<String>,
}

impl FileDiscoveryResult {
    /// Returns true if there are any content mismatch errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Discover and read document files from config without requiring any locks.
///
/// This function performs all file I/O upfront so that lock acquisition
/// for registration can be brief. Returns the file data ready for registration,
/// along with any content mismatch errors found.
///
/// Files in the `documents` config are expected to contain executable definitions
/// (operations, fragments). If schema definitions are found, an error is reported.
pub fn discover_document_files(
    config: &graphql_config::ProjectConfig,
    workspace_path: &std::path::Path,
) -> FileDiscoveryResult {
    let Some(documents_config) = &config.documents else {
        return FileDiscoveryResult::default();
    };

    let patterns: Vec<String> = documents_config
        .patterns()
        .into_iter()
        .map(std::string::ToString::to_string)
        .collect();

    let mut result = FileDiscoveryResult::default();

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

                                        // Validate content matches expected kind (Executable)
                                        // For TS/JS files, we need to extract GraphQL first
                                        let graphql_content = if language.requires_extraction() {
                                            // Extract and concatenate all GraphQL blocks
                                            let config = graphql_extract::ExtractConfig::default();
                                            graphql_extract::extract_from_source(
                                                &content, language, &config, &path_str,
                                            )
                                            .unwrap_or_default()
                                            .iter()
                                            .map(|block| block.source.as_str())
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                        } else {
                                            content.clone()
                                        };

                                        // Skip files that require extraction but contain no GraphQL
                                        if language.requires_extraction()
                                            && graphql_content.is_empty()
                                        {
                                            continue;
                                        }

                                        // Check for schema definitions in document files
                                        if let Some(mismatch) =
                                            graphql_syntax::validate_content_matches_kind(
                                                &graphql_content,
                                                DocumentKind::Executable,
                                            )
                                        {
                                            let definitions = match mismatch {
                                                graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema { definitions } => definitions,
                                                graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable { .. } => Vec::new(),
                                            };
                                            result.errors.push(ContentMismatchError {
                                                pattern: pattern.clone(),
                                                file_path: path.clone(),
                                                expected: graphql_config::FileType::Document,
                                                unexpected_definitions: definitions,
                                            });
                                        }

                                        pattern_matched_any_files = true;
                                        result.files.push(DiscoveredFile {
                                            path: file_path,
                                            content,
                                            language,
                                            document_kind,
                                        });
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
            result.unmatched_patterns.push(pattern.clone());
        }
    }

    result
}

/// Expand brace patterns like `{ts,tsx}` into multiple patterns
///
/// This is needed because the glob crate doesn't support brace expansion.
/// For example, `**/*.{ts,tsx}` expands to `["**/*.ts", "**/*.tsx"]`.
pub(crate) fn expand_braces(pattern: &str) -> Vec<String> {
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

/// Check if a path has a given extension (case-insensitive)
pub(crate) fn has_extension(path: &str, ext: &str) -> bool {
    path.len() > ext.len()
        && path.as_bytes()[path.len() - ext.len()..].eq_ignore_ascii_case(ext.as_bytes())
}

/// Determine `FileKind` for a document file based on its path.
///
/// This is used for files loaded from the `documents` configuration.
/// - `.ts`/`.tsx` files -> TypeScript
/// - `.js`/`.jsx` files -> JavaScript
/// - `.graphql`/`.gql` files -> `ExecutableGraphQL`
///
/// Note: Files from the `schema` configuration are always `Language::GraphQL, DocumentKind::Schema`,
/// regardless of their extension.
pub(crate) fn determine_document_file_kind(path: &str, _content: &str) -> (Language, DocumentKind) {
    if has_extension(path, ".ts") || has_extension(path, ".tsx") {
        (Language::TypeScript, DocumentKind::Executable)
    } else if has_extension(path, ".js") || has_extension(path, ".jsx") {
        (Language::JavaScript, DocumentKind::Executable)
    } else {
        (Language::GraphQL, DocumentKind::Executable)
    }
}

/// Convert a filesystem path to a `FilePath` (URI format)
pub(crate) fn path_to_file_path(path: &std::path::Path) -> FilePath {
    let uri_string = path_to_file_uri(path);
    FilePath::new(uri_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_document_files_skips_ts_without_graphql() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // TS file WITH GraphQL
        std::fs::write(
            src_dir.join("with-graphql.ts"),
            r#"
import { gql } from '@apollo/client';
const query = gql`
  query GetUser {
    user { id name }
  }
`;
"#,
        )
        .unwrap();

        // TS file WITHOUT GraphQL
        std::fs::write(
            src_dir.join("no-graphql.ts"),
            r#"
export function add(a: number, b: number): number {
  return a + b;
}
"#,
        )
        .unwrap();

        // Plain .graphql file
        std::fs::write(
            src_dir.join("query.graphql"),
            "query GetUser { user { id } }",
        )
        .unwrap();

        let config = graphql_config::ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Patterns(vec![
                "src/**/*.ts".to_string(),
                "src/**/*.graphql".to_string(),
            ])),
            include: None,
            exclude: None,
            extensions: None,
        };

        let result = discover_document_files(&config, temp_dir.path());

        // Should only discover files that actually contain GraphQL:
        // - with-graphql.ts (has gql tag)
        // - query.graphql (pure GraphQL file)
        // NOT no-graphql.ts (no GraphQL content)
        assert_eq!(
            result.files.len(),
            2,
            "Expected 2 files (1 TS with GraphQL + 1 .graphql), got {}. Files: {:?}",
            result.files.len(),
            result
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>()
        );
    }
}
