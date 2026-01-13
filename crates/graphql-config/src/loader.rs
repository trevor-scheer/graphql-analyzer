use crate::{ConfigError, GraphQLConfig, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Config file names to search for, in order of preference
const CONFIG_FILES: &[&str] = &[
    ".graphqlrc.yml",
    ".graphqlrc.yaml",
    ".graphqlrc.json",
    ".graphqlrc",
    "graphql.config.yml",
    "graphql.config.yaml",
    "graphql.config.json",
];

/// Find a GraphQL config file by walking up the directory tree from the given start directory.
/// Returns the path to the config file if found.
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

        checked_dirs += 1;
        if !current_dir.pop() {
            tracing::debug!(checked_dirs, "No config file found");
            break;
        }
    }

    Ok(None)
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
#[tracing::instrument(skip(contents), fields(path = %path.display(), size = contents.len()))]
pub fn load_config_from_str(contents: &str, path: &Path) -> Result<GraphQLConfig> {
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
        "json" => {
            tracing::trace!("Parsing as JSON");
            parse_json(contents, path)?
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
    serde_yaml::from_str(contents).map_err(|e| ConfigError::Invalid {
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
}
