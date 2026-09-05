use crate::ExitCode;
use anyhow::{Context, Result};
use colored::Colorize;
use graphql_config::{
    extension_namespace_warnings, find_config, load_config, GraphQLConfig, ProjectConfig,
    CONFIG_FILES,
};
use std::path::PathBuf;

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
                .ok_or_else(|| {
                    let searched_files = CONFIG_FILES
                        .iter()
                        .map(|f| format!("  - {f}"))
                        .chain(std::iter::once(
                            "  - package.json (with \"graphql\" key)".to_string(),
                        ))
                        .collect::<Vec<_>>()
                        .join("\n");
                    anyhow::anyhow!(
                        "No GraphQL config file found\n\n\
                        Searched for these files (walking up from {dir}):\n\
                        {searched_files}\n\n\
                        To get started, create a {example} file:\n\n\
                        {sample}\n\n\
                        For more options, see: https://the-guild.dev/graphql/config/docs",
                        dir = current_dir.display(),
                        example = ".graphqlrc.yaml".cyan(),
                        sample = "\
schema: \"schema.graphql\"\n\
documents: \"src/**/*.graphql\"",
                    )
                })?
        };

        let config = load_config(&config_path).context("Failed to load config")?;

        // Surface silent-config-drop warnings (e.g. `extensions.lint:` placed
        // outside the `graphql-analyzer:` namespace, which the loader ignores
        // without complaint). The full file/lint validators run later in the
        // pipeline; we deliberately only print the namespacing warning here so
        // we don't double-report errors that downstream code will also surface.
        for warning in extension_namespace_warnings(&config) {
            eprintln!("{} {}", "warning:".yellow().bold(), warning.message());
        }

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

    /// Look up a project by name with a helpful error listing available projects.
    ///
    /// Use this instead of manually calling `config.projects().find(...)` to get
    /// consistent, actionable error messages across all commands.
    pub fn get_project_config(&self, project_name: Option<&str>) -> Result<ProjectConfig> {
        let selected_name = Self::get_project_name(project_name);
        self.config
            .projects()
            .find(|(name, _)| *name == selected_name)
            .map(|(_, cfg)| cfg.clone())
            .ok_or_else(|| {
                let available: Vec<_> = self
                    .config
                    .projects()
                    .map(|(name, _)| format!("  - {name}"))
                    .collect();
                anyhow::anyhow!(
                    "Project '{selected_name}' not found in config\n\nAvailable projects:\n{}",
                    available.join("\n")
                )
            })
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
        let config = GraphQLConfig::Single(Box::new(ProjectConfig::new(
            SchemaConfig::Path("schema.graphql".to_string()),
            None,
            None,
            None,
            None,
        )));

        // Should work - single project configs don't require --project
        assert!(!config.is_multi_project());
    }

    #[test]
    fn test_multiproject_with_default_allows_no_flag() {
        let mut projects = HashMap::new();
        projects.insert(
            "default".to_string(),
            ProjectConfig::new(
                SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                None,
            ),
        );
        projects.insert(
            "other".to_string(),
            ProjectConfig::new(
                SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                None,
            ),
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
            ProjectConfig::new(
                SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                None,
            ),
        );
        projects.insert(
            "web".to_string(),
            ProjectConfig::new(
                SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                None,
            ),
        );

        let config = GraphQLConfig::Multi { projects };

        // Multi-project config without "default" project
        assert!(config.is_multi_project());
        assert!(!config.projects().any(|(name, _)| name == "default"));
    }
}
