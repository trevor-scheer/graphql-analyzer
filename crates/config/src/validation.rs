//! Configuration validation module.
//!
//! Provides validation for GraphQL configuration files, returning structured
//! errors that can be easily converted to diagnostics by consumers.

use crate::suggestions::did_you_mean;
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

/// Severity level for a validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// Context for validating lint configuration.
///
/// Passed by callers that know about the linter (e.g., the LSP) since the
/// config crate cannot depend on the linter crate.
pub struct LintValidationContext<'a> {
    pub valid_rule_names: &'a [&'a str],
    pub valid_presets: &'a [&'a str],
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
    /// A file's content doesn't match its expected type from the config.
    ///
    /// For example, a file matched by a `schema` pattern contains operations
    /// or fragments, or a file matched by `documents` contains type definitions.
    ContentMismatch {
        /// The project name (empty string for single-project configs).
        project: String,
        /// The pattern that matched this file.
        pattern: String,
        /// Whether the file was expected to be schema or document.
        expected: FileType,
        /// Path to the file with mismatched content.
        file_path: PathBuf,
        /// Names of definitions that don't belong.
        unexpected_definitions: Vec<String>,
    },
    /// A file pattern matched no files on disk.
    UnmatchedPattern {
        project: String,
        pattern: String,
        file_type: FileType,
    },
    /// No files at all were found for a schema or documents section.
    NoFilesFound {
        project: String,
        file_type: FileType,
    },
    /// An unknown lint rule name was specified in config.
    UnknownLintRule {
        project: String,
        rule_name: String,
        suggestion: Option<String>,
    },
    /// An unknown preset name was specified in config.
    UnknownPreset {
        project: String,
        preset_name: String,
        suggestion: Option<String>,
    },
}

impl ConfigValidationError {
    /// Returns the error code for this validation error.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::OverlappingPattern { .. } => "overlapping-files",
            Self::ContentMismatch { .. } => "content-mismatch",
            Self::UnmatchedPattern { .. } => "unmatched-pattern",
            Self::NoFilesFound { .. } => "no-files-found",
            Self::UnknownLintRule { .. } => "unknown-lint-rule",
            Self::UnknownPreset { .. } => "unknown-preset",
        }
    }

    /// Returns the severity level for this validation error.
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Self::UnmatchedPattern { .. } | Self::NoFilesFound { .. } => Severity::Warning,
            Self::OverlappingPattern { .. }
            | Self::ContentMismatch { .. }
            | Self::UnknownLintRule { .. }
            | Self::UnknownPreset { .. } => Severity::Error,
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
            Self::ContentMismatch {
                expected,
                file_path,
                unexpected_definitions,
                ..
            } => {
                let file_name = file_path.file_name().map_or_else(
                    || file_path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );

                let opposite = match expected {
                    FileType::Schema => "executable definitions (operations/fragments)",
                    FileType::Document => "schema definitions (types/interfaces/etc.)",
                };

                if unexpected_definitions.is_empty() {
                    format!("File '{file_name}' in {expected} config contains {opposite}")
                } else {
                    let defs = unexpected_definitions.join(", ");
                    format!("File '{file_name}' in {expected} config contains {opposite}: {defs}")
                }
            }
            Self::UnmatchedPattern { pattern, .. } => {
                format!("Pattern '{pattern}' matched no files.")
            }
            Self::NoFilesFound {
                project, file_type, ..
            } => {
                format!("No {file_type} files found for project '{project}'.")
            }
            Self::UnknownLintRule {
                rule_name,
                suggestion,
                ..
            } => match suggestion {
                Some(s) => format!("Unknown lint rule: '{rule_name}'. Did you mean '{s}'?"),
                None => format!("Unknown lint rule: '{rule_name}'."),
            },
            Self::UnknownPreset {
                preset_name,
                suggestion,
                ..
            } => match suggestion {
                Some(s) => format!("Unknown lint preset: '{preset_name}'. Did you mean '{s}'?"),
                None => format!("Unknown lint preset: '{preset_name}'."),
            },
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
            Self::ContentMismatch { pattern, .. } | Self::UnmatchedPattern { pattern, .. } => {
                find_pattern_location(config_content, pattern, 0)
            }
            Self::NoFilesFound {
                project, file_type, ..
            } => {
                let key = file_type.to_string();
                find_key_location(config_content, project, &key)
            }
            Self::UnknownLintRule { rule_name, .. } => {
                find_pattern_location(config_content, rule_name, 0)
            }
            Self::UnknownPreset { preset_name, .. } => {
                find_pattern_location(config_content, preset_name, 0)
            }
        }
    }
}

/// Validate a GraphQL configuration.
///
/// Returns a list of all validation errors found. An empty list means the
/// configuration is valid.
///
/// When `lint_context` is provided, lint rule names and preset names are
/// validated against the known set from the linter.
#[must_use]
pub fn validate(
    config: &GraphQLConfig,
    workspace_path: &Path,
    lint_context: Option<&LintValidationContext<'_>>,
) -> Vec<ConfigValidationError> {
    let mut errors = Vec::new();
    errors.extend(validate_file_uniqueness(config, workspace_path));
    errors.extend(validate_unmatched_patterns(config, workspace_path));
    if let Some(ctx) = lint_context {
        errors.extend(validate_lint_config(config, ctx));
    }
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

/// Find the location of a key (like `schema:` or `documents:`) within a project section.
fn find_key_location(config_content: &str, project_name: &str, key: &str) -> Option<Location> {
    let mut in_project = project_name == "default";
    let key_with_colon = format!("{key}:");

    for (line_num, line) in config_content.lines().enumerate() {
        let trimmed = line.trim();

        if !in_project {
            if trimmed.starts_with(&format!("{project_name}:"))
                || trimmed.starts_with(&format!("\"{project_name}\":"))
            {
                in_project = true;
            }
            continue;
        }

        if let Some(col) = line.find(&key_with_colon) {
            return Some(Location {
                line: line_num as u32,
                start_column: col as u32,
                end_column: (col + key.len()) as u32,
            });
        }
    }
    None
}

/// Validate that file patterns actually match files on disk.
fn validate_unmatched_patterns(
    config: &GraphQLConfig,
    workspace_path: &Path,
) -> Vec<ConfigValidationError> {
    let mut errors = Vec::new();

    for (project_name, project_config) in config.projects() {
        // Schema patterns
        let mut any_schema_matched = false;
        for pattern in project_config.schema.paths() {
            if pattern.starts_with("http://") || pattern.starts_with("https://") {
                any_schema_matched = true;
                continue;
            }

            let files = resolve_pattern_to_files(pattern, workspace_path);
            if files.is_empty() {
                errors.push(ConfigValidationError::UnmatchedPattern {
                    project: project_name.to_string(),
                    pattern: pattern.to_string(),
                    file_type: FileType::Schema,
                });
            } else {
                any_schema_matched = true;
            }
        }

        if !any_schema_matched {
            errors.push(ConfigValidationError::NoFilesFound {
                project: project_name.to_string(),
                file_type: FileType::Schema,
            });
        }

        // Document patterns
        if let Some(documents_config) = &project_config.documents {
            let mut any_docs_matched = false;
            for pattern in documents_config.patterns() {
                if pattern.trim().starts_with('!') {
                    continue;
                }

                let files = resolve_pattern_to_files(pattern, workspace_path);
                if files.is_empty() {
                    errors.push(ConfigValidationError::UnmatchedPattern {
                        project: project_name.to_string(),
                        pattern: pattern.to_string(),
                        file_type: FileType::Document,
                    });
                } else {
                    any_docs_matched = true;
                }
            }

            if !any_docs_matched {
                errors.push(ConfigValidationError::NoFilesFound {
                    project: project_name.to_string(),
                    file_type: FileType::Document,
                });
            }
        }
    }

    errors
}

/// Validate lint configuration against known rule names and presets.
fn validate_lint_config(
    config: &GraphQLConfig,
    ctx: &LintValidationContext<'_>,
) -> Vec<ConfigValidationError> {
    let mut errors = Vec::new();
    let rule_set: HashSet<&str> = ctx.valid_rule_names.iter().copied().collect();
    let preset_set: HashSet<&str> = ctx.valid_presets.iter().copied().collect();

    for (project_name, project_config) in config.projects() {
        let Some(lint_value) = project_config.lint() else {
            continue;
        };

        validate_lint_value(
            lint_value,
            project_name,
            &rule_set,
            &preset_set,
            &mut errors,
        );
    }

    errors
}

fn validate_lint_value(
    value: &serde_json::Value,
    project_name: &str,
    rule_set: &HashSet<&str>,
    preset_set: &HashSet<&str>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let suggest_preset = |name: &str| -> Option<String> {
        did_you_mean(name, preset_set.iter().copied()).map(String::from)
    };
    let suggest_rule = |name: &str| -> Option<String> {
        did_you_mean(name, rule_set.iter().copied()).map(String::from)
    };

    match value {
        // `lint: "recommended"` — a single preset name
        serde_json::Value::String(name) => {
            if !preset_set.contains(name.as_str()) {
                errors.push(ConfigValidationError::UnknownPreset {
                    suggestion: suggest_preset(name),
                    project: project_name.to_string(),
                    preset_name: name.clone(),
                });
            }
        }
        // `lint: [recommended, strict]` — array of preset names
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(name) = item.as_str() {
                    if !preset_set.contains(name) {
                        errors.push(ConfigValidationError::UnknownPreset {
                            suggestion: suggest_preset(name),
                            project: project_name.to_string(),
                            preset_name: name.to_string(),
                        });
                    }
                }
            }
        }
        // `lint: { extends: ..., rules: { ... } }` — full config object
        serde_json::Value::Object(obj) => {
            // Validate extends field (presets)
            if let Some(extends) = obj.get("extends") {
                match extends {
                    serde_json::Value::String(name) => {
                        if !preset_set.contains(name.as_str()) {
                            errors.push(ConfigValidationError::UnknownPreset {
                                suggestion: suggest_preset(name),
                                project: project_name.to_string(),
                                preset_name: name.clone(),
                            });
                        }
                    }
                    serde_json::Value::Array(items) => {
                        for item in items {
                            if let Some(name) = item.as_str() {
                                if !preset_set.contains(name) {
                                    errors.push(ConfigValidationError::UnknownPreset {
                                        suggestion: suggest_preset(name),
                                        project: project_name.to_string(),
                                        preset_name: name.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Validate rule names
            if let Some(serde_json::Value::Object(rules)) = obj.get("rules") {
                for rule_name in rules.keys() {
                    if !rule_set.contains(rule_name.as_str()) {
                        errors.push(ConfigValidationError::UnknownLintRule {
                            suggestion: suggest_rule(rule_name),
                            project: project_name.to_string(),
                            rule_name: rule_name.clone(),
                        });
                    }
                }
            }
        }
        _ => {}
    }
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
    use std::collections::HashMap as StdHashMap;
    use std::io::Write;
    use tempfile::TempDir;

    #[allow(clippy::unnecessary_wraps)]
    fn lint_extensions(
        lint_value: serde_json::Value,
    ) -> Option<StdHashMap<String, serde_json::Value>> {
        let mut map = StdHashMap::new();
        map.insert("lint".to_string(), lint_value);
        Some(map)
    }

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
                    ProjectConfig::new(
                        SchemaConfig::Path("schema/*.graphql".to_string()),
                        None,
                        None,
                        None,
                        None,
                    ),
                );
                map.insert(
                    "project2".to_string(),
                    ProjectConfig::new(
                        SchemaConfig::Path("schema/*.graphql".to_string()),
                        None,
                        None,
                        None,
                        None,
                    ),
                );
                map
            },
        };

        let errors = validate(&config, workspace_path, None);

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
                    ProjectConfig::new(
                        SchemaConfig::Path("schema1/*.graphql".to_string()),
                        None,
                        None,
                        None,
                        None,
                    ),
                );
                map.insert(
                    "project2".to_string(),
                    ProjectConfig::new(
                        SchemaConfig::Path("schema2/*.graphql".to_string()),
                        None,
                        None,
                        None,
                        None,
                    ),
                );
                map
            },
        };

        let errors = validate(&config, workspace_path, None);
        assert!(errors.is_empty(), "Should not detect any errors");
    }

    #[test]
    fn test_error_location_in_yaml() {
        let config_content = r"
projects:
  github:
    schema: test-workspace/github/schema/*.graphql
    documents: test-workspace/github/operations/*.graphql
";

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
        let config_content = r"
projects:
  github:
    schema: schema/*.graphql
  admin:
    schema: schema/*.graphql
";

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

    #[test]
    fn test_severity_levels() {
        let error = ConfigValidationError::OverlappingPattern {
            project: "test".to_string(),
            pattern: "*.graphql".to_string(),
            file_type: FileType::Schema,
            overlapping_files: vec![],
            occurrence: 0,
        };
        assert_eq!(error.severity(), Severity::Error);

        let error = ConfigValidationError::UnmatchedPattern {
            project: "test".to_string(),
            pattern: "*.graphql".to_string(),
            file_type: FileType::Schema,
        };
        assert_eq!(error.severity(), Severity::Warning);

        let error = ConfigValidationError::NoFilesFound {
            project: "test".to_string(),
            file_type: FileType::Schema,
        };
        assert_eq!(error.severity(), Severity::Warning);

        let error = ConfigValidationError::UnknownLintRule {
            project: "test".to_string(),
            rule_name: "badRule".to_string(),
            suggestion: None,
        };
        assert_eq!(error.severity(), Severity::Error);

        let error = ConfigValidationError::UnknownPreset {
            project: "test".to_string(),
            preset_name: "badPreset".to_string(),
            suggestion: None,
        };
        assert_eq!(error.severity(), Severity::Error);
    }

    #[test]
    fn test_unmatched_pattern_detected() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("nonexistent/*.graphql".to_string()),
            None,
            None,
            None,
            None,
        )));

        let errors = validate(&config, workspace_path, None);
        let unmatched: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unmatched-pattern")
            .collect();
        assert_eq!(unmatched.len(), 1);
        assert!(unmatched[0].message().contains("nonexistent/*.graphql"));

        let no_files: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "no-files-found")
            .collect();
        assert_eq!(no_files.len(), 1);
    }

    #[test]
    fn test_unmatched_pattern_skips_urls() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("https://example.com/graphql".to_string()),
            None,
            None,
            None,
            None,
        )));

        let errors = validate(&config, workspace_path, None);
        let unmatched: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unmatched-pattern")
            .collect();
        assert!(unmatched.is_empty());
    }

    #[test]
    fn test_unknown_lint_rule() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create a schema file so we don't get unmatched-pattern noise
        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let lint_value = serde_json::json!({
            "rules": {
                "knownRule": "error",
                "badRule": "warn"
            }
        });

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(lint_value),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &["knownRule"],
            valid_presets: &["recommended"],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let unknown: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-lint-rule")
            .collect();
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].message().contains("badRule"));
    }

    #[test]
    fn test_unknown_preset() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(serde_json::json!("nonexistent")),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &[],
            valid_presets: &["recommended"],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let unknown: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-preset")
            .collect();
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].message().contains("nonexistent"));
    }

    #[test]
    fn test_valid_lint_config_no_errors() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(serde_json::json!({
                "extends": "recommended",
                "rules": { "myRule": "error" }
            })),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &["myRule"],
            valid_presets: &["recommended"],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let lint_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-lint-rule" || e.code() == "unknown-preset")
            .collect();
        assert!(lint_errors.is_empty());
    }

    #[test]
    fn test_unknown_preset_in_extends_array() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(serde_json::json!({
                "extends": ["recommended", "strict"],
                "rules": {}
            })),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &[],
            valid_presets: &["recommended"],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let unknown: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-preset")
            .collect();
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].message().contains("strict"));
    }

    #[test]
    fn test_unknown_rule_with_suggestion() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let lint_value = serde_json::json!({
            "rules": {
                "noAnonymous": "error"
            }
        });

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(lint_value),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &["noAnonymousOperations", "noDeprecatedUsage"],
            valid_presets: &[],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let unknown: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-lint-rule")
            .collect();
        assert_eq!(unknown.len(), 1);
        assert!(
            unknown[0]
                .message()
                .contains("Did you mean 'noAnonymousOperations'?"),
            "Expected suggestion in message, got: {}",
            unknown[0].message()
        );
    }

    #[test]
    fn test_unknown_preset_with_suggestion() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(serde_json::json!("recomended")),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &[],
            valid_presets: &["recommended", "strict"],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let unknown: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-preset")
            .collect();
        assert_eq!(unknown.len(), 1);
        assert!(
            unknown[0].message().contains("Did you mean 'recommended'?"),
            "Expected suggestion in message, got: {}",
            unknown[0].message()
        );
    }

    #[test]
    fn test_unknown_rule_no_suggestion_for_unrelated_name() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let mut f = std::fs::File::create(workspace_path.join("schema.graphql")).unwrap();
        writeln!(f, "type Query {{ hello: String }}").unwrap();

        let lint_value = serde_json::json!({
            "rules": {
                "totallyWrong": "error"
            }
        });

        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            lint_extensions(lint_value),
        )));

        let ctx = LintValidationContext {
            valid_rule_names: &["noAnonymousOperations"],
            valid_presets: &[],
        };

        let errors = validate(&config, workspace_path, Some(&ctx));
        let unknown: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "unknown-lint-rule")
            .collect();
        assert_eq!(unknown.len(), 1);
        // Should NOT contain a suggestion since the input is too different
        assert!(
            !unknown[0].message().contains("Did you mean"),
            "Should not suggest for unrelated name, got: {}",
            unknown[0].message()
        );
    }

    #[test]
    fn test_find_key_location() {
        let config_content = r"
projects:
  myapp:
    schema: schema.graphql
    documents: src/**/*.graphql
";

        let loc = find_key_location(config_content, "myapp", "schema");
        assert!(loc.is_some());
        let loc = loc.unwrap();
        assert_eq!(loc.line, 3);

        let loc = find_key_location(config_content, "myapp", "documents");
        assert!(loc.is_some());
        let loc = loc.unwrap();
        assert_eq!(loc.line, 4);
    }
}
