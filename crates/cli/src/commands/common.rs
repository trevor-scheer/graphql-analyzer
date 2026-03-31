use crate::ExitCode;
use anyhow::{Context, Result};
use colored::Colorize;
use graphql_config::{find_config, load_config, GraphQLConfig};
use std::path::PathBuf;

/// Fix behavior for commands that support `--fix` / `--fix-dry-run`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixMode {
    /// Do not fix anything (default)
    Off,
    /// Apply fixes to files on disk
    Apply,
    /// Show what would be fixed without writing changes
    DryRun,
}

/// Parsed stdin input for commands that support `--stdin`
#[derive(Debug, Clone)]
pub struct StdinInput {
    /// The content read from stdin
    pub content: String,
    /// The virtual filename used for project resolution and diagnostics
    pub filename: PathBuf,
}

impl StdinInput {
    /// Read stdin content and resolve the virtual filename.
    ///
    /// If `stdin_filename` is provided, it is resolved relative to the current directory.
    /// Otherwise, defaults to `<cwd>/stdin.graphql`.
    pub fn read(stdin_filename: Option<String>) -> Result<Self> {
        use std::io::Read;

        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .context("Failed to read from stdin")?;

        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let filename = match stdin_filename {
            Some(name) => {
                let p = PathBuf::from(&name);
                if p.is_absolute() {
                    p
                } else {
                    cwd.join(p)
                }
            }
            None => cwd.join("stdin.graphql"),
        };

        Ok(Self { content, filename })
    }
}

/// Common context for all CLI commands that require config and project selection
pub struct CommandContext {
    pub config: GraphQLConfig,
    pub base_dir: PathBuf,
}

impl CommandContext {
    /// Load and validate config for a command.
    ///
    /// Enforces --project requirement for multi-project configs, unless a project
    /// named "default" exists. When a multi-project config has a "default" project,
    /// that project will be automatically selected if --project is omitted.
    ///
    /// This allows users to have convenience defaults while still supporting
    /// multiple projects in a single config file.
    pub fn load(
        config_path: Option<PathBuf>,
        project_name: Option<&str>,
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

        // Validate project requirement for multi-project configs.
        // Special case: Allow omitting --project if there's a "default" project,
        // which will be automatically selected (see get_project_name).
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
                ExitCode::ConfigError.exit();
            }
        }

        // Get the base directory from the config path
        let base_dir = config_path
            .parent()
            .context("Failed to get config directory")?
            .to_path_buf();

        Ok(Self { config, base_dir })
    }

    /// Get the project name to use based on user input.
    ///
    /// Returns the requested project name if provided, otherwise returns "default".
    /// This method should only be called after `load()` has validated that either:
    /// - The config is single-project (in which case "default" is the only project)
    /// - The config is multi-project with a "default" project
    /// - The config is multi-project and a project name was explicitly provided
    #[must_use]
    pub fn get_project_name(requested: Option<&str>) -> &str {
        requested.unwrap_or("default")
    }
}

#[cfg(test)]
mod tests {
    use graphql_config::{GraphQLConfig, ProjectConfig, SchemaConfig};
    use std::collections::HashMap;

    #[test]
    fn test_single_project_config_works_without_project_flag() {
        let config = GraphQLConfig::Single(Box::new(ProjectConfig {
            schema: SchemaConfig::Path("schema.graphql".to_string()),
            documents: None,
            include: None,
            exclude: None,
            extensions: None,
        }));

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
                extensions: None,
            },
        );

        let config = GraphQLConfig::Multi { projects };

        // Multi-project config without "default" project
        assert!(config.is_multi_project());
        assert!(!config.projects().any(|(name, _)| name == "default"));
    }
}
