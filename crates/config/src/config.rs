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
    /// Single project configuration (boxed to reduce enum size)
    Single(Box<ProjectConfig>),
}

impl GraphQLConfig {
    /// Get all projects as an iterator.
    /// For single project configs, yields a single item with name "default".
    #[must_use]
    pub fn projects(&self) -> Box<dyn Iterator<Item = (&str, &ProjectConfig)> + '_> {
        match self {
            Self::Single(config) => Box::new(std::iter::once(("default", config.as_ref()))),
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
            Self::Single(config) if name == "default" => Some(config.as_ref()),
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
    /// For single-project configs, returns the project's lint config from extensions
    /// For multi-project configs, returns None (each project has its own)
    #[must_use]
    pub fn lint_config(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Single(config) => config.lint(),
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
        tracing::debug!(
            "  Project has: exclude={}, include={}, schema={}, documents={}",
            config.exclude.as_ref().map_or(0, Vec::len),
            config.include.as_ref().map_or(0, Vec::len),
            config.schema.paths().len(),
            config.documents.as_ref().map_or(0, |d| d.patterns().len())
        );

        // Check explicit excludes first
        if let Some(ref excludes) = config.exclude {
            tracing::debug!("Checking exclude patterns: {:?}", excludes);
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
            tracing::debug!("Checking include patterns: {:?}", includes);
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

        // Check if file matches schema patterns
        let schema_patterns = config.schema.paths();
        tracing::debug!("Checking schema patterns: {:?}", schema_patterns);
        for pattern in &schema_patterns {
            for expanded in Self::expand_braces(pattern) {
                tracing::debug!("  Testing schema pattern: {}", expanded);
                if let Ok(glob_pattern) = glob::Pattern::new(&expanded) {
                    if glob_pattern.matches(&rel_path_str) {
                        tracing::debug!("    ✓ Matched schema pattern: {}", expanded);
                        return true;
                    }
                }
            }
        }

        // Check if file matches document patterns (if specified)
        if let Some(ref documents) = config.documents {
            let patterns = documents.patterns();
            tracing::debug!("Checking document patterns: {:?}", patterns);
            for pattern in patterns {
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
        }

        // Neither schema nor document patterns matched
        tracing::debug!("No schema or document patterns matched, file excluded");
        false
    }

    /// Normalize a glob pattern for consistent matching
    ///
    /// Handles:
    /// - Leading "./" prefix (removes it)
    /// - Leading "/" prefix (removes it - patterns are relative to workspace)
    /// - Consecutive slashes (collapses to single slash)
    fn normalize_pattern(pattern: &str) -> String {
        let mut normalized = pattern.to_string();

        // Remove leading "./"
        if normalized.starts_with("./") {
            normalized = normalized[2..].to_string();
        }

        // Remove leading "/" (patterns should be relative)
        if normalized.starts_with('/') {
            normalized = normalized[1..].to_string();
        }

        // Collapse consecutive slashes (but preserve in **/)
        // This is a simple approach - just replace "//" with "/"
        while normalized.contains("//") {
            normalized = normalized.replace("//", "/");
        }

        normalized
    }

    /// Expand brace patterns like "src/**/*.{ts,tsx}" into separate patterns
    /// Also normalizes patterns for consistent matching
    fn expand_braces(pattern: &str) -> Vec<String> {
        // Normalize pattern first
        let normalized = Self::normalize_pattern(pattern);

        // Simple brace expansion - handles single brace group
        if let Some(start) = normalized.find('{') {
            if let Some(end) = normalized.find('}') {
                let before = &normalized[..start];
                let after = &normalized[end + 1..];
                let options = &normalized[start + 1..end];

                return options
                    .split(',')
                    .map(|opt| format!("{}{}{}", before, opt.trim(), after))
                    .collect();
            }
        }

        vec![normalized]
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

    /// Tool-specific extensions (includes lint configuration)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

impl ProjectConfig {
    /// Get the lint configuration from extensions.
    ///
    /// Lint configuration should be specified under `extensions.lint`:
    /// ```yaml
    /// extensions:
    ///   lint:
    ///     extends: recommended
    ///     rules:
    ///       noDeprecated: warn
    /// ```
    #[must_use]
    pub fn lint(&self) -> Option<&serde_json::Value> {
        self.extensions.as_ref().and_then(|ext| ext.get("lint"))
    }
}

/// Schema source configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaConfig {
    /// Single file path or glob pattern
    Path(String),
    /// Multiple file paths or glob patterns
    Paths(Vec<String>),
    /// Introspection configuration for remote schemas
    Introspection(IntrospectionSchemaConfig),
}

/// Configuration for introspecting a remote GraphQL endpoint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionSchemaConfig {
    /// The GraphQL endpoint URL to introspect
    pub url: String,

    /// HTTP headers to include in the introspection request (e.g., for authentication)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    /// Request timeout in seconds (default: 30)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,

    /// Number of retry attempts on failure (default: 0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
}

impl SchemaConfig {
    /// Get all schema paths/patterns as a slice
    /// For introspection configs, returns an empty vec (use `introspection_config()` instead)
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Self::Path(path) => vec![path.as_str()],
            Self::Paths(paths) => paths.iter().map(String::as_str).collect(),
            Self::Introspection(_) => vec![],
        }
    }

    /// Check if this schema config contains URLs (HTTP/HTTPS) or is an introspection config
    #[must_use]
    pub fn has_remote_schema(&self) -> bool {
        match self {
            Self::Introspection(_) => true,
            _ => self
                .paths()
                .iter()
                .any(|p| p.starts_with("http://") || p.starts_with("https://")),
        }
    }

    /// Get the introspection configuration if this is an introspection schema config
    #[must_use]
    pub fn introspection_config(&self) -> Option<&IntrospectionSchemaConfig> {
        match self {
            Self::Introspection(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is an introspection configuration
    #[must_use]
    pub const fn is_introspection(&self) -> bool {
        matches!(self, Self::Introspection(_))
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
        let config = GraphQLConfig::Single(Box::new(ProjectConfig {
            schema: SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(DocumentsConfig::Pattern("**/*.graphql".to_string())),
            include: None,
            exclude: None,
            extensions: None,
        }));

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

        let config = GraphQLConfig::Single(Box::new(ProjectConfig {
            schema: SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(DocumentsConfig::Pattern("**/*.graphql".to_string())),
            include: None,
            exclude: None,
            extensions: None,
        }));

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

    #[test]
    fn test_pattern_normalization() {
        // Test leading "./" removal
        assert_eq!(
            GraphQLConfig::normalize_pattern("./src/**/*.ts"),
            "src/**/*.ts"
        );

        // Test leading "/" removal
        assert_eq!(
            GraphQLConfig::normalize_pattern("/src/**/*.ts"),
            "src/**/*.ts"
        );

        // Test consecutive slash collapsing
        assert_eq!(
            GraphQLConfig::normalize_pattern("src//components/*.ts"),
            "src/components/*.ts"
        );

        // Test combined normalization
        assert_eq!(
            GraphQLConfig::normalize_pattern("./src//components/*.ts"),
            "src/components/*.ts"
        );

        // Test pattern without issues
        assert_eq!(
            GraphQLConfig::normalize_pattern("src/**/*.ts"),
            "src/**/*.ts"
        );
    }

    #[test]
    fn test_pattern_normalization_with_leading_dot_slash() {
        use std::path::PathBuf;

        let mut projects = HashMap::new();
        projects.insert(
            "web".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema.graphql".to_string()),
                // Pattern with leading "./" should be normalized
                documents: Some(DocumentsConfig::Pattern(
                    "./apps/web/**/*.{ts,tsx}".to_string(),
                )),
                include: None,
                exclude: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };
        let workspace_root = PathBuf::from("/workspace");

        // File path WITHOUT leading "./" should match pattern WITH "./"
        let component_file = PathBuf::from("/workspace/apps/web/src/components/Foo.tsx");
        assert_eq!(
            config.find_project_for_document(&component_file, &workspace_root),
            Some("web")
        );

        let api_file = PathBuf::from("/workspace/apps/web/src/api/client.ts");
        assert_eq!(
            config.find_project_for_document(&api_file, &workspace_root),
            Some("web")
        );
    }

    #[test]
    fn test_schema_files_match_project() {
        use std::path::PathBuf;

        // Simulates a GitHub-style project where:
        // - schema files are in "schema/*.graphql"
        // - document files are in "src/**/*.graphql"
        let mut projects = HashMap::new();
        projects.insert(
            "github".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema/*.graphql".to_string()),
                documents: Some(DocumentsConfig::Patterns(vec![
                    "src/**/*.graphql".to_string(),
                    "src/**/*.ts".to_string(),
                ])),
                include: None,
                exclude: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };
        let workspace_root = PathBuf::from("/workspace");

        // Schema files should match via schema patterns
        let schema_file = PathBuf::from("/workspace/schema/organizations.graphql");
        assert_eq!(
            config.find_project_for_document(&schema_file, &workspace_root),
            Some("github"),
            "Schema files should be matched by schema patterns"
        );

        // Document files should still match via document patterns
        let query_file = PathBuf::from("/workspace/src/queries/user.graphql");
        assert_eq!(
            config.find_project_for_document(&query_file, &workspace_root),
            Some("github"),
            "Query files should be matched by document patterns"
        );

        // Files outside both schema and document patterns should not match
        let other_file = PathBuf::from("/workspace/other/file.graphql");
        assert_eq!(
            config.find_project_for_document(&other_file, &workspace_root),
            None,
            "Files outside schema and document patterns should not match"
        );
    }

    #[test]
    fn test_introspection_schema_config() {
        let yaml = r#"
schema:
  url: https://api.example.com/graphql
  headers:
    Authorization: Bearer token
    X-API-Key: my-key
  timeout: 60
  retry: 3
documents: "**/*.graphql"
"#;
        let config: ProjectConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.schema.is_introspection());
        assert!(config.schema.has_remote_schema());
        assert!(config.schema.paths().is_empty());

        let introspection = config.schema.introspection_config().unwrap();
        assert_eq!(introspection.url, "https://api.example.com/graphql");
        assert_eq!(introspection.timeout, Some(60));
        assert_eq!(introspection.retry, Some(3));

        let headers = introspection.headers.as_ref().unwrap();
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
        assert_eq!(headers.get("X-API-Key"), Some(&"my-key".to_string()));
    }

    #[test]
    fn test_introspection_schema_config_minimal() {
        let yaml = r"
schema:
  url: https://api.example.com/graphql
";
        let config: ProjectConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.schema.is_introspection());
        let introspection = config.schema.introspection_config().unwrap();
        assert_eq!(introspection.url, "https://api.example.com/graphql");
        assert!(introspection.headers.is_none());
        assert!(introspection.timeout.is_none());
        assert!(introspection.retry.is_none());
    }

    #[test]
    fn test_introspection_remote_detection() {
        let introspection = SchemaConfig::Introspection(IntrospectionSchemaConfig {
            url: "https://api.example.com/graphql".to_string(),
            headers: None,
            timeout: None,
            retry: None,
        });
        assert!(introspection.has_remote_schema());
        assert!(introspection.is_introspection());
    }

    #[test]
    fn test_local_schema_not_introspection() {
        let local = SchemaConfig::Path("schema.graphql".to_string());
        assert!(!local.is_introspection());
        assert!(local.introspection_config().is_none());
    }
}
