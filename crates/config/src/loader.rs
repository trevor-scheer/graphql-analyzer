use crate::{ConfigError, GraphQLConfig, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Wrapper to apply environment variable interpolation to config contents.
/// Errors in interpolation are converted to `ConfigError::Invalid`.
fn apply_env_interpolation(contents: &str, path: &Path) -> Result<String> {
    crate::env::interpolate_env_vars(contents).map_err(|e| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: format!("Environment variable interpolation failed: {e}"),
    })
}

/// Config file names to search for, in order of preference
const CONFIG_FILES: &[&str] = &[
    ".graphqlrc.yml",
    ".graphqlrc.yaml",
    ".graphqlrc.json",
    ".graphqlrc.toml",
    ".graphqlrc",
    "graphql.config.yml",
    "graphql.config.yaml",
    "graphql.config.json",
    "graphql.config.toml",
];

/// Find a GraphQL config file by walking up the directory tree from the given start directory.
/// Returns the path to the config file if found.
///
/// Checks for dedicated config files first (`.graphqlrc.*`, `graphql.config.*`),
/// then falls back to `package.json` with a `"graphql"` key.
#[tracing::instrument(fields(start = %start_dir.display()))]
pub fn find_config(start_dir: &Path) -> Result<Option<PathBuf>> {
    let mut current_dir = start_dir.to_path_buf();
    let mut checked_dirs = 0;

    loop {
        tracing::trace!(dir = %current_dir.display(), "Checking directory for config files");
        for file_name in CONFIG_FILES {
            let config_path = current_dir.join(file_name);
            if config_path.exists() && config_path.is_file() {
                tracing::info!(path = %config_path.display(), checked_dirs, "Found config file");
                return Ok(Some(config_path));
            }
        }

        // Check package.json with "graphql" key (lowest priority)
        let package_json_path = current_dir.join("package.json");
        if package_json_path.exists() && package_json_path.is_file() {
            if let Ok(contents) = fs::read_to_string(&package_json_path) {
                if has_graphql_key(&contents) {
                    tracing::info!(
                        path = %package_json_path.display(),
                        checked_dirs,
                        "Found config in package.json"
                    );
                    return Ok(Some(package_json_path));
                }
            }
        }

        checked_dirs += 1;
        if !current_dir.pop() {
            tracing::debug!(checked_dirs, "No config file found");
            break;
        }
    }

    Ok(None)
}

/// Quick check if a JSON string (package.json) contains a "graphql" key.
fn has_graphql_key(contents: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(contents)
        .ok()
        .and_then(|v| v.get("graphql").cloned())
        .is_some()
}

/// Load a GraphQL config from the specified path.
/// Automatically detects the format based on file extension.
#[tracing::instrument(fields(path = %path.display()))]
pub fn load_config(path: &Path) -> Result<GraphQLConfig> {
    tracing::debug!("Reading config file");
    let contents = fs::read_to_string(path)?;
    let config = load_config_from_str(&contents, path)?;
    tracing::info!(
        projects = config.project_count(),
        multi_project = config.is_multi_project(),
        "Config loaded successfully"
    );
    Ok(config)
}

/// Load a GraphQL config from a string.
/// The path is used for error messages and format detection.
///
/// Environment variables in the format `${VAR}` or `${VAR:default}` are
/// interpolated before parsing. This matches graphql-config standard behavior.
#[tracing::instrument(skip(contents), fields(path = %path.display(), size = contents.len()))]
pub fn load_config_from_str(contents: &str, path: &Path) -> Result<GraphQLConfig> {
    let contents = apply_env_interpolation(contents, path)?;
    let contents = contents.as_str();
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");

    tracing::debug!(extension, file_name, "Detecting config format");

    let config = match extension {
        "yml" | "yaml" => {
            tracing::trace!("Parsing as YAML");
            parse_yaml(contents, path)?
        }
        "json" if file_name == "package.json" => {
            tracing::trace!("Extracting graphql config from package.json");
            parse_package_json(contents, path)?
        }
        "json" => {
            tracing::trace!("Parsing as JSON");
            parse_json(contents, path)?
        }
        "toml" => {
            tracing::trace!("Parsing as TOML");
            parse_toml(contents, path)?
        }
        "" if file_name == ".graphqlrc" => {
            // .graphqlrc without extension - try YAML first, then JSON
            tracing::trace!("Trying YAML then JSON for .graphqlrc");
            parse_yaml(contents, path).or_else(|_| parse_json(contents, path))?
        }
        _ => return Err(ConfigError::UnsupportedFormat(path.to_path_buf())),
    };

    tracing::debug!("Validating config");
    validate_config(&config, path)?;

    Ok(config)
}

/// Parse YAML configuration
fn parse_yaml(contents: &str, path: &Path) -> Result<GraphQLConfig> {
    serde_yml::from_str(contents).map_err(|e| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: format!("YAML parse error: {e}"),
    })
}

/// Parse JSON configuration
fn parse_json(contents: &str, path: &Path) -> Result<GraphQLConfig> {
    serde_json::from_str(contents).map_err(|e| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: format!("JSON parse error: {e}"),
    })
}

/// Extract and parse GraphQL config from a package.json "graphql" key.
fn parse_package_json(contents: &str, path: &Path) -> Result<GraphQLConfig> {
    let package: serde_json::Value =
        serde_json::from_str(contents).map_err(|e| ConfigError::Invalid {
            path: path.to_path_buf(),
            message: format!("JSON parse error: {e}"),
        })?;

    let graphql_value = package.get("graphql").ok_or_else(|| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: "package.json does not contain a \"graphql\" key".to_string(),
    })?;

    serde_json::from_value(graphql_value.clone()).map_err(|e| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: format!("Invalid GraphQL config in package.json: {e}"),
    })
}

/// Parse TOML configuration
fn parse_toml(contents: &str, path: &Path) -> Result<GraphQLConfig> {
    toml::from_str(contents).map_err(|e| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: format!("TOML parse error: {e}"),
    })
}

/// Validate the loaded configuration
#[tracing::instrument(skip(config, path), fields(path = %path.display(), projects = config.project_count()))]
fn validate_config(config: &GraphQLConfig, path: &Path) -> Result<()> {
    for (project_name, project_config) in config.projects() {
        tracing::trace!(project = project_name, "Validating project config");

        // Check if schema config is either introspection or has paths
        if project_config.schema.is_introspection() {
            // Introspection config is valid (url validation happens at runtime)
            tracing::trace!(project = project_name, "Schema uses introspection");
        } else {
            // Path-based schema - validate paths are present and non-empty
            let schema_paths = project_config.schema.paths();
            if schema_paths.is_empty() {
                return Err(ConfigError::Invalid {
                    path: path.to_path_buf(),
                    message: format!("Project '{project_name}' has empty schema configuration"),
                });
            }

            for schema_path in schema_paths {
                if schema_path.trim().is_empty() {
                    return Err(ConfigError::Invalid {
                        path: path.to_path_buf(),
                        message: format!("Project '{project_name}' has empty schema path"),
                    });
                }
            }
        }

        if let Some(ref documents) = project_config.documents {
            let doc_patterns = documents.patterns();
            if doc_patterns.is_empty() {
                return Err(ConfigError::Invalid {
                    path: path.to_path_buf(),
                    message: format!("Project '{project_name}' has empty documents configuration"),
                });
            }

            for pattern in doc_patterns {
                if pattern.trim().is_empty() {
                    return Err(ConfigError::Invalid {
                        path: path.to_path_buf(),
                        message: format!("Project '{project_name}' has empty document pattern"),
                    });
                }
            }
        }
    }

    tracing::debug!("Config validation passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_yaml_single_project() {
        let yaml = r#"
schema: "schema.graphql"
documents: "**/*.graphql"
"#;

        let mut file = NamedTempFile::with_suffix(".yml").unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(!config.is_multi_project());
        assert_eq!(config.project_count(), 1);
    }

    #[test]
    fn test_load_yaml_multi_project() {
        let yaml = r#"
projects:
  frontend:
    schema: "frontend/schema.graphql"
    documents: "frontend/**/*.ts"
  backend:
    schema: "backend/schema.graphql"
    documents: "backend/**/*.graphql"
"#;

        let mut file = NamedTempFile::with_suffix(".yml").unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(config.is_multi_project());
        assert_eq!(config.project_count(), 2);
    }

    #[test]
    fn test_load_json_single_project() {
        let json = r#"
{
  "schema": "schema.graphql",
  "documents": "**/*.graphql"
}
"#;

        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        file.write_all(json.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(!config.is_multi_project());
    }

    #[test]
    fn test_validation_empty_schema() {
        let yaml = r#"
schema: ""
"#;

        let mut file = NamedTempFile::with_suffix(".yml").unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let result = load_config(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_find_config_in_current_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".graphqlrc.yml");
        fs::write(&config_path, "schema: schema.graphql").unwrap();

        let found = find_config(temp_dir.path()).unwrap();
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_find_config_in_parent_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".graphqlrc.yml");
        fs::write(&config_path, "schema: schema.graphql").unwrap();

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();

        let found = find_config(&sub_dir).unwrap();
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_find_config_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let found = find_config(temp_dir.path()).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn test_config_file_priority() {
        let temp_dir = tempfile::tempdir().unwrap();

        fs::write(
            temp_dir.path().join(".graphqlrc.yml"),
            "schema: yml.graphql",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("graphql.config.json"),
            r#"{"schema": "json.graphql"}"#,
        )
        .unwrap();

        let found = find_config(temp_dir.path()).unwrap().unwrap();

        assert_eq!(found.file_name().unwrap(), ".graphqlrc.yml");
    }

    #[test]
    fn test_validation_introspection_config() {
        let yaml = r#"
schema:
  url: https://api.example.com/graphql
  timeout: 30
  retry: 2
documents: "**/*.graphql"
"#;

        let mut file = NamedTempFile::with_suffix(".yml").unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(!config.is_multi_project());

        let project = config.get_project("default").unwrap();
        assert!(project.schema.is_introspection());
    }

    #[test]
    fn test_validation_introspection_config_minimal() {
        let yaml = r"
schema:
  url: https://api.example.com/graphql
";

        let mut file = NamedTempFile::with_suffix(".yml").unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(!config.is_multi_project());

        let project = config.get_project("default").unwrap();
        assert!(project.schema.is_introspection());
    }

    #[test]
    fn test_find_config_in_package_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let package_path = temp_dir.path().join("package.json");
        fs::write(
            &package_path,
            r#"{"name": "my-app", "graphql": {"schema": "schema.graphql"}}"#,
        )
        .unwrap();

        let found = find_config(temp_dir.path()).unwrap();
        assert_eq!(found, Some(package_path));
    }

    #[test]
    fn test_package_json_without_graphql_key_skipped() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{"name": "my-app", "version": "1.0.0"}"#,
        )
        .unwrap();

        let found = find_config(temp_dir.path()).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn test_dedicated_config_takes_priority_over_package_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join(".graphqlrc.yml"),
            "schema: dedicated.graphql",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{"name": "my-app", "graphql": {"schema": "package.graphql"}}"#,
        )
        .unwrap();

        let found = find_config(temp_dir.path()).unwrap().unwrap();
        assert_eq!(found.file_name().unwrap(), ".graphqlrc.yml");
    }

    #[test]
    fn test_load_config_from_package_json() {
        let json = r#"{"name": "my-app", "graphql": {"schema": "schema.graphql", "documents": "src/**/*.graphql"}}"#;
        let path = Path::new("package.json");

        let config = load_config_from_str(json, path).unwrap();
        assert!(!config.is_multi_project());
        assert_eq!(config.project_count(), 1);

        let project = config.get_project("default").unwrap();
        assert_eq!(project.schema.paths(), vec!["schema.graphql"]);
    }

    #[test]
    fn test_load_config_from_package_json_multi_project() {
        let json = r#"{"name": "my-app", "graphql": {"projects": {"api": {"schema": "api/schema.graphql"}, "web": {"schema": "web/schema.graphql"}}}}"#;
        let path = Path::new("package.json");

        let config = load_config_from_str(json, path).unwrap();
        assert!(config.is_multi_project());
        assert_eq!(config.project_count(), 2);
    }

    #[test]
    fn test_env_var_interpolation_in_config() {
        let path = std::path::Path::new("test.yml");

        // Set env var for test
        std::env::set_var("GRAPHQL_TEST_URL", "https://api.example.com/graphql");
        let yaml = r#"
schema:
  url: ${GRAPHQL_TEST_URL}
  headers:
    Authorization: "Bearer ${GRAPHQL_TEST_TOKEN:default-token}"
"#;
        let config = load_config_from_str(yaml, path).unwrap();
        let project = config.get_project("default").unwrap();
        let introspection = project.schema.introspection_config().unwrap();
        assert_eq!(introspection.url, "https://api.example.com/graphql");
        let headers = introspection.headers.as_ref().unwrap();
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer default-token".to_string())
        );
        // Clean up
        std::env::remove_var("GRAPHQL_TEST_URL");
    }

    #[test]
    fn test_env_var_interpolation_missing_var_errors() {
        let path = std::path::Path::new("test.yml");
        let yaml = "schema:\n  url: ${GRAPHQL_DEFINITELY_MISSING_VAR_12345}\n";
        let result = load_config_from_str(yaml, path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_toml_single_project() {
        let toml_content = r#"
schema = "schema.graphql"
documents = "**/*.graphql"
"#;

        let mut file = NamedTempFile::with_suffix(".toml").unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(!config.is_multi_project());
        assert_eq!(config.project_count(), 1);

        let project = config.get_project("default").unwrap();
        assert_eq!(project.schema.paths(), vec!["schema.graphql"]);
    }

    #[test]
    fn test_load_toml_multi_project() {
        let toml_content = r#"
[projects.frontend]
schema = "frontend/schema.graphql"
documents = "frontend/**/*.ts"

[projects.backend]
schema = "backend/schema.graphql"
documents = "backend/**/*.graphql"
"#;

        let mut file = NamedTempFile::with_suffix(".toml").unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(config.is_multi_project());
        assert_eq!(config.project_count(), 2);
    }

    #[test]
    fn test_load_toml_with_extensions() {
        let toml_content = r#"
schema = "schema.graphql"

[extensions]
client = "apollo"
lint = "recommended"
"#;

        let mut file = NamedTempFile::with_suffix(".toml").unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config(file.path()).unwrap();
        let project = config.get_project("default").unwrap();
        assert!(project.extensions.is_some());
    }

    #[test]
    fn test_find_config_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".graphqlrc.toml");
        fs::write(&config_path, "schema = \"schema.graphql\"").unwrap();

        let found = find_config(temp_dir.path()).unwrap();
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_yaml_takes_priority_over_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join(".graphqlrc.yml"),
            "schema: yml.graphql",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join(".graphqlrc.toml"),
            "schema = \"toml.graphql\"",
        )
        .unwrap();

        let found = find_config(temp_dir.path()).unwrap().unwrap();
        assert_eq!(found.file_name().unwrap(), ".graphqlrc.yml");
    }
}
