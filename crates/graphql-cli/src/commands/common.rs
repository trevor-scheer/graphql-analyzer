use anyhow::{Context, Result};
use colored::Colorize;
use graphql_config::{find_config, load_config, GraphQLConfig};
use std::path::PathBuf;
use std::process;

/// Common context for all CLI commands that require config and project selection
pub struct CommandContext {
    pub config: GraphQLConfig,
    pub base_dir: PathBuf,
}

impl CommandContext {
    /// Load and validate config for a command.
    /// Enforces --project requirement for multi-project configs.
    pub fn load(
        config_path: Option<PathBuf>,
        project_name: Option<&String>,
        command_name: &str,
    ) -> Result<Self> {
        // Find and load config
        let config_path = if let Some(path) = config_path {
            path
        } else {
            let current_dir = std::env::current_dir()?;
            find_config(&current_dir)
                .context("Failed to search for config")?
                .context("No GraphQL config file found")?
        };

        let config = load_config(&config_path).context("Failed to load config")?;

        // Validate project requirement for multi-project configs
        // Allow omitting --project if there's a "default" project
        if config.is_multi_project() && project_name.is_none() {
            // Check if there's a "default" project
            let has_default = config.projects().any(|(name, _)| name == "default");

            if !has_default {
                eprintln!(
                    "{}",
                    "Error: Multi-project configuration requires --project flag".red()
                );
                eprintln!("\nAvailable projects:");
                for (name, _) in config.projects() {
                    eprintln!("  - {}", name.cyan());
                }
                eprintln!(
                    "\nUsage: {} --project <NAME> {}",
                    "graphql".green(),
                    command_name
                );
                process::exit(1);
            }
        }

        // Get the base directory from the config path
        let base_dir = config_path
            .parent()
            .context("Failed to get config directory")?
            .to_path_buf();

        Ok(Self { config, base_dir })
    }
}

#[cfg(test)]
mod tests {
    use graphql_config::{GraphQLConfig, ProjectConfig, SchemaConfig};
    use std::collections::HashMap;

    #[test]
    fn test_single_project_config_works_without_project_flag() {
        let config = GraphQLConfig::Single(ProjectConfig {
            schema: SchemaConfig::Path("schema.graphql".to_string()),
            documents: None,
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        });

        // Should work - single project configs don't require --project
        assert!(!config.is_multi_project());
    }

    #[test]
    fn test_multiproject_with_default_allows_no_flag() {
        let mut projects = HashMap::new();
        projects.insert(
            "default".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema.graphql".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );
        projects.insert(
            "other".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema.graphql".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };

        // Multi-project config with "default" project
        assert!(config.is_multi_project());
        assert!(config.projects().any(|(name, _)| name == "default"));
    }

    #[test]
    fn test_multiproject_without_default_requires_flag() {
        let mut projects = HashMap::new();
        projects.insert(
            "api".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema.graphql".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );
        projects.insert(
            "web".to_string(),
            ProjectConfig {
                schema: SchemaConfig::Path("schema.graphql".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };

        // Multi-project config without "default" project
        assert!(config.is_multi_project());
        assert!(!config.projects().any(|(name, _)| name == "default"));
    }
}
