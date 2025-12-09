use crate::{ProjectError, Result};
use graphql_config::SchemaConfig;
use std::path::Path;

/// Schema loader for loading GraphQL schemas from various sources
pub struct SchemaLoader {
    config: SchemaConfig,
    base_path: Option<std::path::PathBuf>,
}

impl SchemaLoader {
    #[must_use]
    pub const fn new(config: SchemaConfig) -> Self {
        Self {
            config,
            base_path: None,
        }
    }

    #[must_use]
    pub fn with_base_path(mut self, path: impl AsRef<Path>) -> Self {
        self.base_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Load schema files with their paths for proper source tracking
    #[tracing::instrument(skip(self), fields(paths = self.config.paths().len()))]
    pub async fn load_with_paths(&self) -> Result<Vec<(String, String)>> {
        tracing::info!("Loading schema files");

        // Include Apollo Client built-in directives
        const APOLLO_CLIENT_BUILTINS: &str = include_str!("apollo_client_builtins.graphql");

        let mut schema_files = Vec::new();
        schema_files.push((
            "apollo_client_builtins.graphql".to_string(),
            APOLLO_CLIENT_BUILTINS.to_string(),
        ));

        for path in self.config.paths() {
            if path.starts_with("http://") || path.starts_with("https://") {
                // Remote schema via introspection
                tracing::info!(url = path, "Loading remote schema");
                let schema = self.load_remote(path).await?;
                schema_files.push((path.to_string(), schema));
            } else {
                // Local file(s) - may include globs
                tracing::debug!(pattern = path, "Loading local schema file(s)");
                let files = self.load_local_with_paths(path)?;
                tracing::debug!(files_loaded = files.len(), "Local files loaded");
                schema_files.extend(files);
            }
        }

        if schema_files.is_empty() {
            return Err(ProjectError::SchemaLoad(
                "No schema files found".to_string(),
            ));
        }

        tracing::info!(
            total_files = schema_files.len(),
            "Schema files loaded successfully"
        );
        Ok(schema_files)
    }

    /// Load schema as a string
    pub async fn load(&self) -> Result<String> {
        let files = self.load_with_paths().await?;
        Ok(files
            .into_iter()
            .map(|(_, content)| content)
            .collect::<Vec<_>>()
            .join("\n\n"))
    }

    /// Load schema from local file(s) with paths, supporting glob patterns
    fn load_local_with_paths(&self, pattern: &str) -> Result<Vec<(String, String)>> {
        let pattern = self.base_path.as_ref().map_or_else(
            || pattern.to_string(),
            |base| base.join(pattern).display().to_string(),
        );

        let mut schema_files = Vec::new();

        // Try as glob pattern first
        match glob::glob(&pattern) {
            Ok(paths) => {
                let mut found_any = false;
                for entry in paths {
                    match entry {
                        Ok(path) => {
                            found_any = true;
                            let content = std::fs::read_to_string(&path)?;
                            let path_str = path.display().to_string();
                            schema_files.push((path_str, content));
                        }
                        Err(e) => {
                            return Err(ProjectError::SchemaLoad(format!("Glob error: {e}")));
                        }
                    }
                }

                if !found_any {
                    return Err(ProjectError::SchemaLoad(format!(
                        "No files matched pattern: {pattern}"
                    )));
                }
            }
            Err(e) => {
                return Err(ProjectError::SchemaLoad(format!(
                    "Invalid glob pattern '{pattern}': {e}"
                )));
            }
        }

        Ok(schema_files)
    }

    /// Load schema from remote endpoint via introspection
    async fn load_remote(&self, url: &str) -> Result<String> {
        let sdl = graphql_introspect::introspect_url_to_sdl(url)
            .await
            .map_err(|e| ProjectError::SchemaLoad(format!("Failed to introspect {url}: {e}")))?;
        Ok(sdl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_config::SchemaConfig;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_load_single_local_file() {
        let temp_dir = tempdir().unwrap();
        let schema_path = temp_dir.path().join("schema.graphql");
        fs::write(&schema_path, "type Query { hello: String }").unwrap();

        let config = SchemaConfig::Path(schema_path.display().to_string());
        let loader = SchemaLoader::new(config);
        let schema = loader.load().await.unwrap();

        assert!(schema.contains("type Query"));
    }

    #[tokio::test]
    async fn test_load_multiple_files_with_glob() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("schema1.graphql"),
            "type Query { hello: String }",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("schema2.graphql"),
            "type Mutation { hello: String }",
        )
        .unwrap();

        let pattern = temp_dir.path().join("*.graphql").display().to_string();
        let config = SchemaConfig::Path(pattern);
        let loader = SchemaLoader::new(config);
        let schema = loader.load().await.unwrap();

        assert!(schema.contains("type Query"));
        assert!(schema.contains("type Mutation"));
    }
}
