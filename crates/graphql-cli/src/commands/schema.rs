//! Schema-related CLI commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use graphql_config::{find_config, load_config, IntrospectionSchemaConfig};
use graphql_introspect::{introspection_to_sdl, IntrospectionClient};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// Default timeout in seconds for introspection requests.
const DEFAULT_TIMEOUT: u64 = 30;

/// Schema output format.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum SchemaFormat {
    /// SDL (Schema Definition Language) format
    #[default]
    Sdl,
    /// JSON introspection format
    Json,
}

/// Schema subcommands.
#[derive(Subcommand)]
pub enum SchemaCommands {
    /// Download schema from a remote GraphQL endpoint via introspection
    ///
    /// The endpoint can be specified either as a URL argument or loaded from the
    /// GraphQL config file using --project.
    Download {
        /// GraphQL endpoint URL to introspect (optional if --project is used)
        #[arg(value_name = "URL")]
        url: Option<String>,

        /// Path to GraphQL config file
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// Project name to load introspection config from
        #[arg(short, long)]
        project: Option<String>,

        /// Output file path (writes to stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// HTTP headers to include in the request (can be specified multiple times)
        /// Format: "Header-Name: Header-Value"
        /// These are merged with headers from the config file (CLI takes precedence)
        #[arg(long = "header", short = 'H', value_name = "HEADER")]
        headers: Vec<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "sdl")]
        format: SchemaFormat,

        /// Request timeout in seconds (overrides config file)
        #[arg(long)]
        timeout: Option<u64>,

        /// Number of retry attempts on failure (overrides config file)
        #[arg(long)]
        retry: Option<u32>,
    },
}

/// Run a schema subcommand.
pub async fn run(command: SchemaCommands) -> Result<()> {
    match command {
        SchemaCommands::Download {
            url,
            config,
            project,
            output,
            headers,
            format,
            timeout,
            retry,
        } => {
            run_download(
                url, config, project, output, headers, format, timeout, retry,
            )
            .await
        }
    }
}

/// Resolved introspection settings from config file and CLI arguments.
#[derive(Debug)]
struct IntrospectionSettings {
    url: String,
    headers: Vec<(String, String)>,
    timeout: u64,
    retry: u32,
}

/// Load introspection settings from config file.
fn load_from_config(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
) -> Result<IntrospectionSchemaConfig> {
    // Find config file
    let config_path = if let Some(path) = config_path {
        path
    } else {
        let current_dir = std::env::current_dir()?;
        find_config(&current_dir)
            .context("Failed to search for config")?
            .context(
                "No GraphQL config file found. Use --config to specify one or provide a URL.",
            )?
    };

    let config = load_config(&config_path).context("Failed to load config")?;

    // Get project name
    let project_name = project_name.unwrap_or("default");

    // Get project config
    let project_config = config.get_project(project_name).with_context(|| {
        if config.is_multi_project() {
            let available: Vec<_> = config.projects().map(|(name, _)| name).collect();
            format!(
                "Project '{}' not found. Available projects: {}",
                project_name,
                available.join(", ")
            )
        } else {
            format!("Project '{project_name}' not found")
        }
    })?;

    // Check if schema is an introspection config
    project_config
        .schema
        .introspection_config()
        .cloned()
        .with_context(|| {
            format!(
                "Project '{project_name}' does not have an introspection schema config. \
                Expected schema to be an object with 'url' field."
            )
        })
}

/// Parses a header string in "Name: Value" format.
fn parse_header(header: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = header.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid header format: '{header}'. Expected 'Header-Name: Header-Value'");
    }
    let name = parts[0].trim().to_string();
    let value = parts[1].trim().to_string();
    if name.is_empty() {
        anyhow::bail!("Header name cannot be empty");
    }
    Ok((name, value))
}

/// Resolve introspection settings from URL/config and CLI overrides.
fn resolve_settings(
    url: Option<String>,
    config_path: Option<PathBuf>,
    project: Option<&str>,
    cli_headers: &[String],
    cli_timeout: Option<u64>,
    cli_retry: Option<u32>,
) -> Result<IntrospectionSettings> {
    // If URL is provided directly, use it with CLI settings only
    if let Some(url) = url {
        let headers = cli_headers
            .iter()
            .map(|h| parse_header(h))
            .collect::<Result<Vec<_>>>()
            .context("Failed to parse headers")?;

        return Ok(IntrospectionSettings {
            url,
            headers,
            timeout: cli_timeout.unwrap_or(DEFAULT_TIMEOUT),
            retry: cli_retry.unwrap_or(0),
        });
    }

    // No URL provided - must load from config
    if project.is_none() && config_path.is_none() {
        anyhow::bail!(
            "Either a URL argument or --project flag is required.\n\n\
            Usage:\n  \
            graphql schema download <URL>\n  \
            graphql schema download --project <NAME>"
        );
    }

    // Load from config file
    let introspection_config = load_from_config(config_path, project)?;

    // Start with headers from config
    let mut headers: Vec<(String, String)> = introspection_config
        .headers
        .unwrap_or_default()
        .into_iter()
        .collect();

    // Parse and merge CLI headers (CLI takes precedence)
    let cli_parsed: Vec<(String, String)> = cli_headers
        .iter()
        .map(|h| parse_header(h))
        .collect::<Result<Vec<_>>>()
        .context("Failed to parse headers")?;

    for (name, value) in cli_parsed {
        // Remove existing header with same name (case-insensitive)
        headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
        headers.push((name, value));
    }

    Ok(IntrospectionSettings {
        url: introspection_config.url,
        headers,
        // CLI overrides config values
        timeout: cli_timeout.unwrap_or(introspection_config.timeout.unwrap_or(DEFAULT_TIMEOUT)),
        retry: cli_retry.unwrap_or(introspection_config.retry.unwrap_or(0)),
    })
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(cli_headers))]
async fn run_download(
    url: Option<String>,
    config_path: Option<PathBuf>,
    project: Option<String>,
    output: Option<PathBuf>,
    cli_headers: Vec<String>,
    format: SchemaFormat,
    cli_timeout: Option<u64>,
    cli_retry: Option<u32>,
) -> Result<()> {
    let start_time = std::time::Instant::now();

    // Resolve settings from URL/config and CLI overrides
    let settings = resolve_settings(
        url,
        config_path,
        project.as_deref(),
        &cli_headers,
        cli_timeout,
        cli_retry,
    )?;

    // Build the introspection client
    let mut client = IntrospectionClient::new()
        .with_timeout(Duration::from_secs(settings.timeout))
        .with_retries(settings.retry);

    for (name, value) in &settings.headers {
        client = client.with_header(name, value);
    }

    // Show spinner for interactive output
    let spinner = if output.is_some() {
        Some(crate::progress::spinner(&format!(
            "Fetching schema from {}...",
            settings.url
        )))
    } else {
        None // Don't show spinner when writing to stdout
    };

    // Execute introspection
    let content = match format {
        SchemaFormat::Sdl => {
            let response = client
                .execute(&settings.url)
                .await
                .with_context(|| format!("Failed to fetch schema from {}", settings.url))?;
            introspection_to_sdl(&response)
        }
        SchemaFormat::Json => {
            let response = client
                .execute_raw(&settings.url)
                .await
                .with_context(|| format!("Failed to fetch schema from {}", settings.url))?;
            serde_json::to_string_pretty(&response)
                .context("Failed to serialize introspection response")?
        }
    };

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Write output
    if let Some(path) = output {
        std::fs::write(&path, &content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;

        let duration = start_time.elapsed();
        let format_name = match format {
            SchemaFormat::Sdl => "SDL",
            SchemaFormat::Json => "JSON",
        };

        println!(
            "{} Schema downloaded to {} ({} format)",
            "✓".green(),
            path.display().to_string().cyan(),
            format_name
        );
        println!("  {} {:.2}s", "⏱".dimmed(), duration.as_secs_f64());
    } else {
        // Write to stdout
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        handle
            .write_all(content.as_bytes())
            .context("Failed to write to stdout")?;
        // Ensure trailing newline for SDL
        if !content.ends_with('\n') {
            handle.write_all(b"\n").ok();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_valid() {
        let (name, value) = parse_header("Authorization: Bearer token").unwrap();
        assert_eq!(name, "Authorization");
        assert_eq!(value, "Bearer token");
    }

    #[test]
    fn test_parse_header_with_colons_in_value() {
        let (name, value) = parse_header("X-Custom: value:with:colons").unwrap();
        assert_eq!(name, "X-Custom");
        assert_eq!(value, "value:with:colons");
    }

    #[test]
    fn test_parse_header_with_whitespace() {
        let (name, value) = parse_header("  Content-Type  :  application/json  ").unwrap();
        assert_eq!(name, "Content-Type");
        assert_eq!(value, "application/json");
    }

    #[test]
    fn test_parse_header_invalid_no_colon() {
        let result = parse_header("InvalidHeader");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_header_empty_name() {
        let result = parse_header(": value");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_settings_with_url() {
        let headers = vec!["Authorization: Bearer token".to_string()];
        let settings = resolve_settings(
            Some("https://example.com/graphql".to_string()),
            None,
            None,
            &headers,
            Some(60),
            Some(3),
        )
        .unwrap();

        assert_eq!(settings.url, "https://example.com/graphql");
        assert_eq!(settings.headers.len(), 1);
        assert_eq!(settings.headers[0].0, "Authorization");
        assert_eq!(settings.timeout, 60);
        assert_eq!(settings.retry, 3);
    }

    #[test]
    fn test_resolve_settings_defaults() {
        let settings = resolve_settings(
            Some("https://example.com/graphql".to_string()),
            None,
            None,
            &[],
            None,
            None,
        )
        .unwrap();

        assert_eq!(settings.timeout, DEFAULT_TIMEOUT);
        assert_eq!(settings.retry, 0);
    }

    #[test]
    fn test_resolve_settings_requires_url_or_project() {
        let result = resolve_settings(None, None, None, &[], None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Either a URL argument or --project flag is required"));
    }
}
