use crate::commands::common::CommandContext;
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::Write;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};

/// Configuration structure for codegen extension
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodegenConfig {
    /// Output file configurations
    generates: serde_json::Value,

    /// Optional overwrite flag (defaults to true)
    #[serde(default = "default_overwrite")]
    overwrite: bool,

    /// Optional hooks configuration
    #[serde(default)]
    hooks: Option<serde_json::Value>,
}

fn default_overwrite() -> bool {
    true
}

/// Run the codegen command
#[tracing::instrument(skip_all)]
pub fn run(config_path: Option<PathBuf>, project_name: Option<&str>, watch: bool) -> Result<()> {
    let start_time = std::time::Instant::now();

    // Load config
    let ctx = CommandContext::load(config_path, project_name, "codegen")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Get codegen extension configuration
    let codegen_config = project_config
        .extensions
        .as_ref()
        .and_then(|ext| ext.get("codegen"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No codegen configuration found. Add 'extensions.codegen' to your GraphQL config.\n\n\
                Example configuration:\n\
                \n\
                extensions:\n\
                  codegen:\n\
                    generates:\n\
                      src/generated/types.ts:\n\
                        plugins:\n\
                          - typescript\n\
                          - typescript-operations"
            )
        })?;

    // Parse the codegen configuration
    let codegen: CodegenConfig = serde_json::from_value(codegen_config.clone())
        .context("Invalid codegen configuration format")?;

    println!("{}", "Generating TypeScript types...".cyan());

    // Build the full codegen config file content
    // This merges schema/documents from the project config with codegen-specific settings
    let schema_value = match &project_config.schema {
        graphql_config::SchemaConfig::Path(p) => serde_json::json!(p),
        graphql_config::SchemaConfig::Paths(ps) => serde_json::json!(ps),
    };

    let documents_value = project_config.documents.as_ref().map(|docs| match docs {
        graphql_config::DocumentsConfig::Pattern(p) => serde_json::json!(p),
        graphql_config::DocumentsConfig::Patterns(ps) => serde_json::json!(ps),
    });

    let mut full_config = serde_json::json!({
        "schema": schema_value,
        "generates": codegen.generates,
        "overwrite": codegen.overwrite,
    });

    if let Some(docs) = documents_value {
        full_config["documents"] = docs;
    }

    if let Some(hooks) = codegen.hooks {
        full_config["hooks"] = hooks;
    }

    // Write temporary config file
    let temp_config_path = ctx.base_dir.join(".graphql-codegen.temp.yaml");
    let config_yaml =
        serde_yaml::to_string(&full_config).context("Failed to serialize codegen configuration")?;

    {
        let mut file = std::fs::File::create(&temp_config_path)
            .context("Failed to create temporary config file")?;
        file.write_all(config_yaml.as_bytes())
            .context("Failed to write temporary config file")?;
    }

    // Ensure temp file is cleaned up on exit (unless watch mode)
    let cleanup_temp = !watch;

    // Build the command
    let mut cmd = Command::new("npx");
    cmd.arg("graphql-codegen");
    cmd.arg("--config");
    cmd.arg(&temp_config_path);

    if watch {
        cmd.arg("--watch");
    }

    // Set working directory
    cmd.current_dir(&ctx.base_dir);

    // Inherit stdout/stderr for real-time output
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    tracing::debug!("Running command: {:?}", cmd);

    // Execute the command
    let status = cmd.status().context(
        "Failed to execute graphql-codegen. \
         Make sure @graphql-codegen/cli is installed:\n\n  \
         npm install -D @graphql-codegen/cli",
    )?;

    // Clean up temp file if not in watch mode
    if cleanup_temp {
        if let Err(e) = std::fs::remove_file(&temp_config_path) {
            tracing::warn!("Failed to remove temporary config file: {}", e);
        }
    }

    let total_duration = start_time.elapsed();

    if status.success() {
        println!(
            "\n{}",
            "✓ Code generation completed successfully!".green().bold()
        );
        println!(
            "  {} total: {:.2}s",
            "⏱".dimmed(),
            total_duration.as_secs_f64()
        );
        Ok(())
    } else {
        let exit_code = status.code().unwrap_or(1);
        eprintln!(
            "\n{}",
            format!("✗ Code generation failed (exit code: {exit_code})").red()
        );
        process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codegen_config_deserialization() {
        let json = serde_json::json!({
            "generates": {
                "src/generated/types.ts": {
                    "plugins": ["typescript", "typescript-operations"]
                }
            }
        });

        let config: CodegenConfig = serde_json::from_value(json).unwrap();
        assert!(config.overwrite); // default value
        assert!(config.hooks.is_none());
    }

    #[test]
    fn test_codegen_config_with_all_fields() {
        let json = serde_json::json!({
            "generates": {
                "src/generated/types.ts": {
                    "plugins": ["typescript"]
                }
            },
            "overwrite": false,
            "hooks": {
                "afterAllFileWrite": ["prettier --write"]
            }
        });

        let config: CodegenConfig = serde_json::from_value(json).unwrap();
        assert!(!config.overwrite);
        assert!(config.hooks.is_some());
    }
}
