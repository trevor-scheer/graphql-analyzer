//! Fragment usage analysis command
//!
//! Analyzes how fragments are used across the project, identifying
//! unused fragments and showing usage statistics.

use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::process;

#[tracing::instrument(skip(config_path, project_name, format), fields(project = ?project_name))]
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "fragments")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Load project
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let host = CliAnalysisHost::from_project_config(&project_config, &ctx.base_dir)
        .map_err(|e| {
            if matches!(format, OutputFormat::Human) {
                eprintln!("{} {}", "✗ Failed to load project:".red(), e);
            } else {
                eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
            }
            process::exit(1);
        })
        .unwrap();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Get fragment usage analysis
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Analyzing fragment usage..."))
    } else {
        None
    };

    let fragment_usages = host.fragment_usages();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Display results
    match format {
        OutputFormat::Human => {
            display_human_format(&fragment_usages, start_time.elapsed());
        }
        OutputFormat::Json => {
            display_json_format(&fragment_usages);
        }
    }

    Ok(())
}

/// Format a file path for display
/// Strips "file://" prefix and tries to make paths relative to CWD for readability
fn format_path(path: &str) -> String {
    let path = path.strip_prefix("file://").unwrap_or(path);

    // Try to make relative to current directory
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = std::path::Path::new(path).strip_prefix(&cwd) {
            return rel.display().to_string();
        }
    }

    path.to_string()
}

fn display_human_format(
    fragment_usages: &[graphql_ide::FragmentUsage],
    elapsed: std::time::Duration,
) {
    println!();
    println!("{}", "Fragment Usage Report".bold().underline());
    println!("{}", "=====================".dimmed());
    println!();

    if fragment_usages.is_empty() {
        println!("{}", "No fragments found in the project.".yellow());
        return;
    }

    // Count unused fragments
    let unused_count = fragment_usages.iter().filter(|f| f.is_unused()).count();
    let total_count = fragment_usages.len();

    // Sort by usage count (most used first), then by name
    let mut sorted: Vec<_> = fragment_usages.iter().collect();
    sorted.sort_by(|a, b| {
        b.usage_count()
            .cmp(&a.usage_count())
            .then_with(|| a.name.cmp(&b.name))
    });

    // Display each fragment
    for usage in &sorted {
        let count = usage.usage_count();
        let files_count = usage
            .usages
            .iter()
            .map(|r| &r.location.file)
            .collect::<std::collections::HashSet<_>>()
            .len();

        if usage.is_unused() {
            // Highlight unused fragments with a warning
            println!(
                "{} {}: {} {}",
                "⚠".yellow(),
                usage.name.red().bold(),
                "0 usages".red(),
                "(consider removing)".dimmed()
            );
        } else {
            let usage_text = if count == 1 {
                "1 usage in 1 file".to_string()
            } else if files_count == 1 {
                format!("{count} usages in 1 file")
            } else {
                format!("{count} usages in {files_count} files")
            };

            println!("{}: {}", usage.name.green().bold(), usage_text);
        }

        // Show definition location
        let def_line = usage.definition_range.start.line + 1; // 1-based for display
        println!(
            "  {} {}:{}",
            "defined at:".dimmed(),
            format_path(usage.definition_file.as_str()).cyan(),
            def_line.to_string().cyan()
        );

        // Show usage locations
        if !usage.usages.is_empty() {
            println!("  {}", "used at:".dimmed());
            for reference in &usage.usages {
                let ref_line = reference.location.range.start.line + 1; // 1-based for display
                println!(
                    "    {} {}:{}",
                    "•".dimmed(),
                    format_path(reference.location.file.as_str()),
                    ref_line
                );
            }
        }

        // Show transitive dependencies if any
        if !usage.transitive_dependencies.is_empty() {
            println!(
                "  {} {}",
                "→ depends on:".dimmed(),
                usage.transitive_dependencies.join(", ").dimmed()
            );
        }

        println!(); // Blank line between fragments
    }

    // Summary
    println!();
    println!("{}", "─".repeat(40).dimmed());

    if unused_count > 0 {
        println!(
            "{} {} of {} fragments are unused",
            "⚠".yellow(),
            unused_count.to_string().yellow().bold(),
            total_count
        );
    } else {
        println!(
            "{} All {} fragments are in use",
            "✓".green(),
            total_count.to_string().green().bold()
        );
    }

    println!(
        "  {} completed in {:.2}s",
        "⏱".dimmed(),
        elapsed.as_secs_f64()
    );
}

fn display_json_format(fragment_usages: &[graphql_ide::FragmentUsage]) {
    #[derive(serde::Serialize)]
    struct JsonOutput {
        fragments: Vec<FragmentJson>,
        summary: SummaryJson,
    }

    #[derive(serde::Serialize)]
    struct FragmentJson {
        name: String,
        definition_file: String,
        usage_count: usize,
        usages: Vec<UsageJson>,
        transitive_dependencies: Vec<String>,
        is_unused: bool,
    }

    #[derive(serde::Serialize)]
    struct UsageJson {
        file: String,
        line: u32,
        column: u32,
    }

    #[derive(serde::Serialize)]
    #[allow(clippy::struct_field_names)] // Field names are semantically correct for JSON output
    struct SummaryJson {
        total_fragments: usize,
        unused_fragments: usize,
        used_fragments: usize,
    }

    let fragments: Vec<FragmentJson> = fragment_usages
        .iter()
        .map(|f| FragmentJson {
            name: f.name.clone(),
            definition_file: f.definition_file.as_str().to_string(),
            usage_count: f.usage_count(),
            usages: f
                .usages
                .iter()
                .map(|u| UsageJson {
                    file: u.location.file.as_str().to_string(),
                    line: u.location.range.start.line + 1, // 1-based for output
                    column: u.location.range.start.character + 1,
                })
                .collect(),
            transitive_dependencies: f.transitive_dependencies.clone(),
            is_unused: f.is_unused(),
        })
        .collect();

    let unused_count = fragment_usages.iter().filter(|f| f.is_unused()).count();

    let output = JsonOutput {
        summary: SummaryJson {
            total_fragments: fragment_usages.len(),
            unused_fragments: unused_count,
            used_fragments: fragment_usages.len() - unused_count,
        },
        fragments,
    };

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_path_strips_file_prefix() {
        let result = format_path("file:///home/user/project/query.graphql");
        assert!(!result.starts_with("file://"));
        assert!(result.contains("query.graphql"));
    }

    #[test]
    fn test_format_path_handles_no_prefix() {
        let result = format_path("/home/user/project/query.graphql");
        assert!(result.contains("query.graphql"));
    }

    #[test]
    fn test_format_path_relative_when_possible() {
        // When CWD matches the path prefix, it should make it relative
        let result = format_path("/some/path/file.graphql");
        // Result should still contain the file name
        assert!(result.contains("file.graphql"));
    }
}
