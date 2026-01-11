//! Schema-related CLI commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use graphql_introspect::{introspection_to_sdl, IntrospectionClient};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

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
    Download {
        /// GraphQL endpoint URL to introspect
        url: String,

        /// Output file path (writes to stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// HTTP headers to include in the request (can be specified multiple times)
        /// Format: "Header-Name: Header-Value"
        #[arg(long = "header", short = 'H', value_name = "HEADER")]
        headers: Vec<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "sdl")]
        format: SchemaFormat,

        /// Request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,

        /// Number of retry attempts on failure
        #[arg(long, default_value = "0")]
        retry: u32,
    },
}

/// Run a schema subcommand.
pub async fn run(command: SchemaCommands) -> Result<()> {
    match command {
        SchemaCommands::Download {
            url,
            output,
            headers,
            format,
            timeout,
            retry,
        } => run_download(url, output, headers, format, timeout, retry).await,
    }
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

#[tracing::instrument(skip(headers))]
async fn run_download(
    url: String,
    output: Option<PathBuf>,
    headers: Vec<String>,
    format: SchemaFormat,
    timeout: u64,
    retry: u32,
) -> Result<()> {
    let start_time = std::time::Instant::now();

    // Parse headers
    let parsed_headers: Vec<(String, String)> = headers
        .iter()
        .map(|h| parse_header(h))
        .collect::<Result<Vec<_>>>()
        .context("Failed to parse headers")?;

    // Build the introspection client
    let mut client = IntrospectionClient::new()
        .with_timeout(Duration::from_secs(timeout))
        .with_retries(retry);

    for (name, value) in &parsed_headers {
        client = client.with_header(name, value);
    }

    // Show spinner for interactive output
    let spinner = if output.is_some() {
        Some(crate::progress::spinner(&format!(
            "Fetching schema from {url}..."
        )))
    } else {
        None // Don't show spinner when writing to stdout
    };

    // Execute introspection
    let content = match format {
        SchemaFormat::Sdl => {
            let response = client
                .execute(&url)
                .await
                .with_context(|| format!("Failed to fetch schema from {url}"))?;
            introspection_to_sdl(&response)
        }
        SchemaFormat::Json => {
            let response = client
                .execute_raw(&url)
                .await
                .with_context(|| format!("Failed to fetch schema from {url}"))?;
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
}
