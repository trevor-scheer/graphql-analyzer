//! Configuration validation module.
//!
//! Provides validation for GraphQL configuration files, returning structured
//! errors that can be easily converted to diagnostics by consumers.

use crate::GraphQLConfig;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A location within a config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: u32,
    pub start_column: u32,
    pub end_column: u32,
}

/// The type of file pattern that caused a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    Schema,
    Document,
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schema => write!(f, "schema"),
            Self::Document => write!(f, "documents"),
        }
    }
}

/// A validation error found in a GraphQL configuration file.
#[derive(Debug, Clone)]
pub enum ConfigValidationError {
    /// A pattern matches files that also belong to other projects.
    OverlappingPattern {
        /// The project name that has the conflicting pattern.
        project: String,
        /// The pattern that matched.
        pattern: String,
        /// Whether this is a schema or document pattern.
        file_type: FileType,
        /// The files matched by this pattern that also belong to other projects.
        overlapping_files: Vec<PathBuf>,
        /// Which occurrence of this pattern in the config file (0-indexed).
        /// Needed when the same pattern appears under multiple projects.
        occurrence: usize,
    },
}

impl ConfigValidationError {
    /// Returns the error code for this validation error.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::OverlappingPattern { .. } => "overlapping-files",
        }
    }

    /// Returns a human-readable error message.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::OverlappingPattern {
                file_type,
                overlapping_files,
                ..
            } => {
                let count = overlapping_files.len();
                if count == 1 {
                    let name = overlapping_files[0].file_name().map_or_else(
                        || "1 file".to_string(),
                        |n| format!("'{}'", n.to_string_lossy()),
                    );
                    format!("This {file_type} pattern causes {name} to belong to multiple projects")
                } else {
                    format!("This {file_type} pattern causes {count} files to belong to multiple projects")
                }
            }
        }
    }

    /// Returns the location of this error in the config file content, if it can be determined.
    #[must_use]
    pub fn location(&self, config_content: &str) -> Option<Location> {
        match self {
            Self::OverlappingPattern {
                pattern,
                occurrence,
                ..
            } => find_pattern_location(config_content, pattern, *occurrence),
        }
    }
}

/// Validate a GraphQL configuration.
///
/// Returns a list of all validation errors found. An empty list means the
/// configuration is valid.
#[must_use]
pub fn validate(config: &GraphQLConfig, workspace_path: &Path) -> Vec<ConfigValidationError> {
    let mut errors = Vec::new();
    errors.extend(validate_file_uniqueness(config, workspace_path));
    errors
}

/// Validate that no file belongs to multiple projects.
fn validate_file_uniqueness(
    config: &GraphQLConfig,
    workspace_path: &Path,
) -> Vec<ConfigValidationError> {
    // Map from canonical file path to list of (project_name, pattern, type)
    let mut file_to_projects: HashMap<PathBuf, Vec<(String, String, FileType)>> = HashMap::new();

    for (project_name, project_config) in config.projects() {
        for pattern in project_config.schema.paths() {
            if pattern.starts_with("http://") || pattern.starts_with("https://") {
                continue;
            }

            for file_path in resolve_pattern_to_files(pattern, workspace_path) {
                file_to_projects.entry(file_path).or_default().push((
                    project_name.to_string(),
                    pattern.to_string(),
                    FileType::Schema,
                ));
            }
        }

        if let Some(documents_config) = &project_config.documents {
            for pattern in documents_config.patterns() {
                if pattern.trim().starts_with('!') {
                    continue;
                }

                for file_path in resolve_pattern_to_files(pattern, workspace_path) {
                    file_to_projects.entry(file_path).or_default().push((
                        project_name.to_string(),
                        pattern.to_string(),
                        FileType::Document,
                    ));
                }
            }
        }
    }

    // Group overlapping files by (project, pattern) so each pattern produces one error
    let mut pattern_to_files: HashMap<(String, String, FileType), Vec<PathBuf>> = HashMap::new();

    for (file_path, matches) in file_to_projects {
        let unique_projects: HashSet<&str> = matches.iter().map(|(p, _, _)| p.as_str()).collect();
        if unique_projects.len() > 1 {
            for (project, pattern, file_type) in matches {
                pattern_to_files
                    .entry((project, pattern, file_type))
                    .or_default()
                    .push(file_path.clone());
            }
        }
    }

    // Create one error per (project, pattern), tracking occurrence index for
    // patterns that appear multiple times in the config file
    let mut pattern_occurrences: HashMap<String, usize> = HashMap::new();
    pattern_to_files
        .into_iter()
        .map(|((project, pattern, file_type), overlapping_files)| {
            let occurrence = *pattern_occurrences.entry(pattern.clone()).or_insert(0);
            *pattern_occurrences.get_mut(&pattern).unwrap() += 1;
            ConfigValidationError::OverlappingPattern {
                project,
                pattern,
                file_type,
                overlapping_files,
                occurrence,
            }
        })
        .collect()
}

/// Find the Nth occurrence of a pattern string in config content.
fn find_pattern_location(
    config_content: &str,
    pattern: &str,
    occurrence: usize,
) -> Option<Location> {
    let mut found = 0;
    for (line_num, line) in config_content.lines().enumerate() {
        if let Some(col) = line.find(pattern) {
            if found == occurrence {
                #[allow(clippy::cast_possible_truncation)]
                return Some(Location {
                    line: line_num as u32,
                    start_column: col as u32,
                    end_column: (col + pattern.len()) as u32,
                });
            }
            found += 1;
        }
    }
    None
}

/// Resolve a glob pattern to actual file paths.
fn resolve_pattern_to_files(pattern: &str, workspace_path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for expanded_pattern in expand_braces(pattern) {
        let full_pattern = workspace_path.join(&expanded_pattern);

        if let Ok(paths) = glob::glob(&full_pattern.display().to_string()) {
            for entry in paths.flatten() {
                if entry.is_file() {
                    // Skip node_modules
                    if entry.components().any(|c| c.as_os_str() == "node_modules") {
                        continue;
                    }

                    if let Ok(canonical) = entry.canonicalize() {
                        files.push(canonical);
                    } else {
                        files.push(entry);
                    }
                }
            }
        }
    }

    files
}

/// Expand brace patterns like `{ts,tsx}` into multiple patterns.
fn expand_braces(pattern: &str) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectConfig, SchemaConfig};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_validate_detects_overlapping_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create schema directory and file
        let schema_dir = workspace_path.join("schema");
        std::fs::create_dir(&schema_dir).unwrap();
        let mut schema_file = std::fs::File::create(schema_dir.join("schema.graphql")).unwrap();
        writeln!(schema_file, "type Query {{ hello: String }}").unwrap();

        let config = GraphQLConfig::Multi {
            projects: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "project1".to_string(),
                    ProjectConfig {
                        schema: SchemaConfig::Path("schema/*.graphql".to_string()),
                        documents: None,
                        include: None,
                        exclude: None,
                        extensions: None,
                        lint: None,
                    },
                );
                map.insert(
                    "project2".to_string(),
                    ProjectConfig {
                        schema: SchemaConfig::Path("schema/*.graphql".to_string()),
                        documents: None,
                        include: None,
                        exclude: None,
                        extensions: None,
                        lint: None,
                    },
                );
                map
            },
        };

        let errors = validate(&config, workspace_path);

        // One error per (project, pattern) — two projects, same pattern
        assert_eq!(
            errors.len(),
            2,
            "Should detect two overlapping patterns: {errors:?}"
        );

        for error in &errors {
            assert_eq!(error.code(), "overlapping-files");
            assert!(error.message().contains("schema.graphql"));
        }
    }

    #[test]
    fn test_validate_no_errors_when_no_overlap() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create separate schema directories
        let schema1_dir = workspace_path.join("schema1");
        std::fs::create_dir(&schema1_dir).unwrap();
        let mut schema1_file = std::fs::File::create(schema1_dir.join("schema.graphql")).unwrap();
        writeln!(schema1_file, "type Query {{ hello: String }}").unwrap();

        let schema2_dir = workspace_path.join("schema2");
        std::fs::create_dir(&schema2_dir).unwrap();
        let mut schema2_file = std::fs::File::create(schema2_dir.join("schema.graphql")).unwrap();
        writeln!(schema2_file, "type Query {{ world: String }}").unwrap();

        let config = GraphQLConfig::Multi {
            projects: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "project1".to_string(),
                    ProjectConfig {
                        schema: SchemaConfig::Path("schema1/*.graphql".to_string()),
                        documents: None,
                        include: None,
                        exclude: None,
                        extensions: None,
                        lint: None,
                    },
                );
                map.insert(
                    "project2".to_string(),
                    ProjectConfig {
                        schema: SchemaConfig::Path("schema2/*.graphql".to_string()),
                        documents: None,
                        include: None,
                        exclude: None,
                        extensions: None,
                        lint: None,
                    },
                );
                map
            },
        };

        let errors = validate(&config, workspace_path);
        assert!(errors.is_empty(), "Should not detect any errors");
    }

    #[test]
    fn test_error_location_in_yaml() {
        let config_content = r#"
projects:
  github:
    schema: test-workspace/github/schema/*.graphql
    documents: test-workspace/github/operations/*.graphql
"#;

        let error = ConfigValidationError::OverlappingPattern {
            project: "github".to_string(),
            pattern: "test-workspace/github/schema/*.graphql".to_string(),
            file_type: FileType::Schema,
            overlapping_files: vec![PathBuf::from("test.graphql")],
            occurrence: 0,
        };

        let location = error.location(config_content);
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.line, 3);
    }

    #[test]
    fn test_error_location_nth_occurrence() {
        let config_content = r#"
projects:
  github:
    schema: schema/*.graphql
  admin:
    schema: schema/*.graphql
"#;

        let first = ConfigValidationError::OverlappingPattern {
            project: "github".to_string(),
            pattern: "schema/*.graphql".to_string(),
            file_type: FileType::Schema,
            overlapping_files: vec![PathBuf::from("test.graphql")],
            occurrence: 0,
        };
        let second = ConfigValidationError::OverlappingPattern {
            project: "admin".to_string(),
            pattern: "schema/*.graphql".to_string(),
            file_type: FileType::Schema,
            overlapping_files: vec![PathBuf::from("test.graphql")],
            occurrence: 1,
        };

        let loc0 = first.location(config_content).unwrap();
        let loc1 = second.location(config_content).unwrap();
        assert_eq!(loc0.line, 3);
        assert_eq!(loc1.line, 5);
    }

    #[test]
    fn test_error_location_in_json() {
        let config_content = r#"{
  "projects": {
    "github": {
      "schema": "test-workspace/github/schema/*.graphql"
    }
  }
}"#;

        let error = ConfigValidationError::OverlappingPattern {
            project: "github".to_string(),
            pattern: "test-workspace/github/schema/*.graphql".to_string(),
            file_type: FileType::Schema,
            overlapping_files: vec![PathBuf::from("test.graphql")],
            occurrence: 0,
        };

        let location = error.location(config_content);
        assert!(location.is_some());
    }
}
