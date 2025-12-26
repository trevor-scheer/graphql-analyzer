use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level GraphQL configuration.
/// Either a single project or multiple named projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GraphQLConfig {
    /// Multi-project configuration
    Multi {
        projects: HashMap<String, ProjectConfig>,
    },
    /// Single project configuration
    Single(ProjectConfig),
}

impl GraphQLConfig {
    /// Get all projects as an iterator.
    /// For single project configs, yields a single item with name "default".
    #[must_use]
    pub fn projects(&self) -> Box<dyn Iterator<Item = (&str, &ProjectConfig)> + '_> {
        match self {
            Self::Single(config) => Box::new(std::iter::once(("default", config))),
            Self::Multi { projects, .. } => Box::new(
                projects
                    .iter()
                    .map(|(name, config)| (name.as_str(), config)),
            ),
        }
    }

    /// Get a specific project by name.
    /// For single project configs, returns the project if name is "default".
    #[must_use]
    pub fn get_project(&self, name: &str) -> Option<&ProjectConfig> {
        match self {
            Self::Single(config) if name == "default" => Some(config),
            Self::Single(_) => None,
            Self::Multi { projects, .. } => projects.get(name),
        }
    }

    /// Check if this is a multi-project configuration
    #[must_use]
    pub const fn is_multi_project(&self) -> bool {
        matches!(self, Self::Multi { .. })
    }

    /// Get the number of projects
    #[must_use]
    pub fn project_count(&self) -> usize {
        match self {
            Self::Single(_) => 1,
            Self::Multi { projects } => projects.len(),
        }
    }

    /// Get lint configuration from the first/default project
    /// For single-project configs, returns the project's lint config
    /// For multi-project configs, returns None (each project has its own)
    #[must_use]
    pub const fn lint_config(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Single(config) => config.lint.as_ref(),
            Self::Multi { .. } => None,
        }
    }

    /// Get extensions from the first/default project
    /// For single-project configs, returns the project's extensions
    /// For multi-project configs, returns None (each project has its own)
    #[must_use]
    pub const fn extensions(&self) -> Option<&HashMap<String, serde_json::Value>> {
        match self {
            Self::Single(config) => config.extensions.as_ref(),
            Self::Multi { .. } => None,
        }
    }

    /// Find the project that a document belongs to based on pattern matching.
    ///
    /// For single-project configs, always returns "default".
    /// For multi-project configs, matches the document path against each project's
    /// document patterns (includes/excludes).
    ///
    /// # Arguments
    /// * `doc_path` - Absolute path to the document
    /// * `workspace_root` - Root directory of the workspace (used to resolve relative patterns)
    ///
    /// # Returns
    /// The name of the matching project, or None if no project matches
    #[must_use]
    pub fn find_project_for_document(
        &self,
        doc_path: &Path,
        workspace_root: &Path,
    ) -> Option<&str> {
        match self {
            Self::Single(_) => Some("default"),
            Self::Multi { projects } => {
                for (name, config) in projects {
                    if Self::document_matches_project(doc_path, workspace_root, config) {
                        return Some(name.as_str());
                    }
                }
                None
            }
        }
    }

    /// Check if a document matches a project's patterns
    fn document_matches_project(
        doc_path: &Path,
        workspace_root: &Path,
        config: &ProjectConfig,
    ) -> bool {
        let Ok(rel_path) = doc_path.strip_prefix(workspace_root) else {
            tracing::debug!("Document not in workspace root");
            return false;
        };

        let rel_path_str = rel_path.to_string_lossy();
        tracing::debug!("Checking if '{}' matches project patterns", rel_path_str);

        // Check explicit excludes first
        if let Some(ref excludes) = config.exclude {
            for pattern in excludes {
                for expanded in Self::expand_braces(pattern) {
                    if let Ok(glob_pattern) = glob::Pattern::new(&expanded) {
                        if glob_pattern.matches(&rel_path_str) {
                            return false;
                        }
                    }
                }
            }
        }

        // Determine if file is in project scope based on include/exclude patterns
        let in_include_scope = config.include.as_ref().is_none_or(|includes| {
            tracing::debug!("Checking include patterns ({} patterns)", includes.len());
            let mut matched = false;
            for pattern in includes {
                for expanded in Self::expand_braces(pattern) {
                    tracing::debug!("  Testing include pattern: {}", expanded);
                    if let Ok(glob_pattern) = glob::Pattern::new(&expanded) {
                        if glob_pattern.matches(&rel_path_str) {
                            tracing::debug!("    ✓ Matched include pattern: {}", expanded);
                            matched = true;
                            break;
                        }
                    }
                }
                if matched {
                    break;
                }
            }
            if !matched {
                tracing::debug!("No include patterns matched, file excluded");
            }
            matched
        });

        // If file is not in include scope, it doesn't match this project
        if !in_include_scope {
            return false;
        }

        // File is in scope - now check if it matches document patterns (if specified)
        if let Some(ref documents) = config.documents {
            tracing::debug!(
                "Checking document patterns ({} patterns)",
                documents.patterns().len()
            );
            for pattern in documents.patterns() {
                for expanded in Self::expand_braces(pattern) {
                    tracing::debug!("  Testing document pattern: {}", expanded);
                    if let Ok(glob_pattern) = glob::Pattern::new(&expanded) {
                        if glob_pattern.matches(&rel_path_str) {
                            tracing::debug!("    ✓ Matched document pattern: {}", expanded);
                            return true;
                        }
                    }
                }
            }
            // Document patterns specified but no match
            tracing::debug!("No document patterns matched, file excluded");
            return false;
        }

        // No document patterns - if file is in include scope, it matches
        tracing::debug!("No document patterns specified, matching by include scope");
        true
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
}

/// Configuration for a single GraphQL project
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    /// Schema source(s)
    pub schema: SchemaConfig,

    /// Document patterns (queries, mutations, fragments)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents: Option<DocumentsConfig>,

    /// File patterns to include
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,

    /// File patterns to exclude
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,

    /// Lint configuration (applies to all tools by default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint: Option<serde_json::Value>,

    /// Tool-specific extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

/// Schema source configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaConfig {
    /// Single file path or glob pattern
    Path(String),
    /// Multiple file paths or glob patterns
    Paths(Vec<String>),
}

impl SchemaConfig {
    /// Get all schema paths/patterns as a slice
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Self::Path(path) => vec![path.as_str()],
            Self::Paths(paths) => paths.iter().map(String::as_str).collect(),
        }
    }

    /// Check if this schema config contains URLs (HTTP/HTTPS)
    #[must_use]
    pub fn has_remote_schema(&self) -> bool {
        self.paths()
            .iter()
            .any(|p| p.starts_with("http://") || p.starts_with("https://"))
    }
}

/// Documents source configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentsConfig {
    /// Single pattern
    Pattern(String),
    /// Multiple patterns
    Patterns(Vec<String>),
}

impl DocumentsConfig {
    /// Get all document patterns as a slice
    #[must_use]
    pub fn patterns(&self) -> Vec<&str> {
        match self {
            Self::Pattern(pattern) => vec![pattern.as_str()],
            Self::Patterns(patterns) => patterns.iter().map(String::as_str).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_project_config() {
        let config = GraphQLConfig::Single(ProjectConfig {
            schema: SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(DocumentsConfig::Pattern("**/*.graphql".to_string())),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        });

        assert!(!config.is_multi_project());
        assert_eq!(config.project_count(), 1);
        assert!(config.get_project("default").is_some());
        assert!(config.get_project("other").is_none());
    }

    #[test]
    fn test_multi_project_config() {
        let mut projects = HashMap::new();
        projects.insert(
            "frontend".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("frontend/schema.graphql".to_string()),
                documents: Some(DocumentsConfig::Pattern("frontend/**/*.ts".to_string())),
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );
        projects.insert(
            "backend".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("backend/schema.graphql".to_string()),
                documents: Some(DocumentsConfig::Pattern("backend/**/*.graphql".to_string())),
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };

        assert!(config.is_multi_project());
        assert_eq!(config.project_count(), 2);
        assert!(config.get_project("frontend").is_some());
        assert!(config.get_project("backend").is_some());
        assert!(config.get_project("default").is_none());
    }

    #[test]
    fn test_schema_config_paths() {
        let single = SchemaConfig::Path("schema.graphql".to_string());
        assert_eq!(single.paths(), vec!["schema.graphql"]);

        let multiple = SchemaConfig::Paths(vec![
            "schema1.graphql".to_string(),
            "schema2.graphql".to_string(),
        ]);
        assert_eq!(multiple.paths(), vec!["schema1.graphql", "schema2.graphql"]);
    }

    #[test]
    fn test_remote_schema_detection() {
        let local = SchemaConfig::Path("schema.graphql".to_string());
        assert!(!local.has_remote_schema());

        let remote = SchemaConfig::Path("https://api.example.com/graphql".to_string());
        assert!(remote.has_remote_schema());

        let mixed = SchemaConfig::Paths(vec![
            "schema.graphql".to_string(),
            "https://api.example.com/graphql".to_string(),
        ]);
        assert!(mixed.has_remote_schema());
    }

    #[test]
    fn test_documents_config_patterns() {
        let single = DocumentsConfig::Pattern("**/*.graphql".to_string());
        assert_eq!(single.patterns(), vec!["**/*.graphql"]);

        let multiple =
            DocumentsConfig::Patterns(vec!["**/*.graphql".to_string(), "**/*.ts".to_string()]);
        assert_eq!(multiple.patterns(), vec!["**/*.graphql", "**/*.ts"]);
    }

    #[test]
    fn test_extensions_field() {
        // Test that extensions field can be deserialized
        let yaml = r#"
schema: schema.graphql
extensions:
  extractConfig:
    magicComment: "MyGraphQL"
    tagIdentifiers: ["myTag"]
  otherExtension:
    someKey: "someValue"
"#;
        let config: ProjectConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.extensions.is_some());
        let extensions = config.extensions.unwrap();
        assert!(extensions.contains_key("extractConfig"));
        assert!(extensions.contains_key("otherExtension"));
    }

    #[test]
    fn test_find_project_single_config() {
        use std::path::PathBuf;

        let config = GraphQLConfig::Single(ProjectConfig {
            schema: SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(DocumentsConfig::Pattern("**/*.graphql".to_string())),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        });

        let workspace_root = PathBuf::from("/workspace");
        let doc_path = PathBuf::from("/workspace/src/queries.graphql");

        let project = config.find_project_for_document(&doc_path, &workspace_root);
        assert_eq!(project, Some("default"));
    }

    #[test]
    fn test_find_project_multi_config_with_documents() {
        use std::path::PathBuf;

        let mut projects = HashMap::new();
        projects.insert(
            "frontend".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("frontend/schema.graphql".to_string()),
                documents: Some(DocumentsConfig::Pattern(
                    "frontend/**/*.{ts,tsx}".to_string(),
                )),
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );
        projects.insert(
            "backend".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("backend/schema.graphql".to_string()),
                documents: Some(DocumentsConfig::Pattern("backend/**/*.graphql".to_string())),
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };
        let workspace_root = PathBuf::from("/workspace");

        let backend_doc = PathBuf::from("/workspace/backend/api.graphql");
        assert_eq!(
            config.find_project_for_document(&backend_doc, &workspace_root),
            Some("backend")
        );

        let frontend_doc = PathBuf::from("/workspace/frontend/components/User.tsx");
        assert_eq!(
            config.find_project_for_document(&frontend_doc, &workspace_root),
            Some("frontend")
        );

        let no_match = PathBuf::from("/workspace/other/file.graphql");
        assert_eq!(
            config.find_project_for_document(&no_match, &workspace_root),
            None
        );
    }

    #[test]
    fn test_find_project_with_include_exclude() {
        use std::path::PathBuf;

        let mut projects = HashMap::new();
        projects.insert(
            "main".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema.graphql".to_string()),
                documents: Some(DocumentsConfig::Pattern("**/*.graphql".to_string())),
                include: Some(vec!["src/**".to_string()]),
                exclude: Some(vec!["**/__tests__/**".to_string()]),
                lint: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };
        let workspace_root = PathBuf::from("/workspace");

        let included = PathBuf::from("/workspace/src/queries.graphql");
        assert_eq!(
            config.find_project_for_document(&included, &workspace_root),
            Some("main")
        );

        let excluded = PathBuf::from("/workspace/src/__tests__/queries.graphql");
        assert_eq!(
            config.find_project_for_document(&excluded, &workspace_root),
            None
        );

        let not_included = PathBuf::from("/workspace/other/queries.graphql");
        assert_eq!(
            config.find_project_for_document(&not_included, &workspace_root),
            None
        );
    }
}
