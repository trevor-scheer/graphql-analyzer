use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

/// Complexity analysis output for JSON format
#[derive(serde::Serialize)]
struct ComplexityOutput {
    operation_name: String,
    operation_type: String,
    file: String,
    total_complexity: u32,
    depth: u32,
    breakdown: Vec<FieldOutput>,
    warnings: Vec<String>,
}

#[derive(serde::Serialize)]
struct FieldOutput {
    path: String,
    complexity: u32,
    multiplier: u32,
    depth: u32,
    is_connection: bool,
    warning: Option<String>,
}

#[allow(clippy::too_many_lines)]
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    threshold: Option<u32>,
    breakdown: bool,
) -> Result<()> {
    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "complexity")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Load and select project
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let load_start = std::time::Instant::now();
    let host = CliAnalysisHost::from_project_config(&project_config, &ctx.base_dir)?;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let load_duration = load_start.elapsed();

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) {
        println!("{}", "✓ Schema loaded successfully".green());
        println!("{}", "✓ Documents loaded successfully".green());
    }

    // Run complexity analysis
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner(
            "Analyzing operation complexity...",
        ))
    } else {
        None
    };

    let analysis_start = std::time::Instant::now();
    let results = host.complexity_analysis();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let analysis_duration = analysis_start.elapsed();

    // Filter by threshold if specified
    let threshold_value = threshold.unwrap_or(0);
    let exceeds_threshold: Vec<_> = results
        .iter()
        .filter(|r| r.total_complexity >= threshold_value)
        .collect();

    // Display results
    match format {
        OutputFormat::Human => {
            if results.is_empty() {
                println!("\n{}", "No operations found to analyze".yellow());
            } else {
                println!("\n{}", "Complexity Analysis Results".bold().underline());
                println!();

                for result in &results {
                    // Operation header
                    let status =
                        if threshold_value > 0 && result.total_complexity >= threshold_value {
                            "⚠".yellow().to_string()
                        } else {
                            "✓".green().to_string()
                        };

                    println!(
                        "{} {} {} ({})",
                        status,
                        result.operation_type.cyan(),
                        result.operation_name.bold(),
                        result.file.as_str().dimmed()
                    );

                    // Complexity metrics
                    println!(
                        "    {} {} | {} {}",
                        "Complexity:".dimmed(),
                        format_complexity(result.total_complexity, threshold_value),
                        "Depth:".dimmed(),
                        result.depth.to_string().cyan()
                    );

                    // Warnings
                    for warning in &result.warnings {
                        println!("    {} {}", "⚠ Warning:".yellow(), warning);
                    }

                    // Field breakdown (if requested)
                    if breakdown && !result.breakdown.is_empty() {
                        println!("    {}:", "Field Breakdown".dimmed());
                        for field in &result.breakdown {
                            let connection_tag = if field.is_connection {
                                " [connection]".yellow().to_string()
                            } else {
                                String::new()
                            };
                            let warning_tag = if let Some(ref w) = field.warning {
                                format!(" ⚠ {}", w.yellow())
                            } else {
                                String::new()
                            };

                            println!(
                                "      {} {} (×{}){}{}",
                                field.path.dimmed(),
                                field.complexity.to_string().cyan(),
                                field.multiplier,
                                connection_tag,
                                warning_tag
                            );
                        }
                    }

                    println!();
                }
            }
        }
        OutputFormat::Json => {
            for result in &results {
                let output = ComplexityOutput {
                    operation_name: result.operation_name.clone(),
                    operation_type: result.operation_type.clone(),
                    file: result.file.as_str().to_string(),
                    total_complexity: result.total_complexity,
                    depth: result.depth,
                    breakdown: result
                        .breakdown
                        .iter()
                        .map(|f| FieldOutput {
                            path: f.path.clone(),
                            complexity: f.complexity,
                            multiplier: f.multiplier,
                            depth: f.depth,
                            is_connection: f.is_connection,
                            warning: f.warning.clone(),
                        })
                        .collect(),
                    warnings: result.warnings.clone(),
                };
                println!("{}", serde_json::to_string(&output)?);
            }
        }
    }

    // Summary
    let total_duration = start_time.elapsed();
    if matches!(format, OutputFormat::Human) {
        let total_ops = results.len();
        let high_complexity = exceeds_threshold.len();

        if threshold_value > 0 {
            if high_complexity > 0 {
                println!(
                    "{}",
                    format!(
                        "⚠ {high_complexity} of {total_ops} operation(s) exceed complexity threshold ({threshold_value})"
                    )
                    .yellow()
                    .bold()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "✓ All {total_ops} operation(s) are within complexity threshold ({threshold_value})"
                    )
                    .green()
                    .bold()
                );
            }
        } else {
            println!("{}", format!("Analyzed {total_ops} operation(s)").bold());
        }

        println!(
            "  {} load: {:.2}s, analysis: {:.2}s, total: {:.2}s",
            "⏱".dimmed(),
            load_duration.as_secs_f64(),
            analysis_duration.as_secs_f64(),
            total_duration.as_secs_f64()
        );
    }

    // Exit with error code if threshold exceeded
    if threshold_value > 0 && !exceeds_threshold.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

/// Format complexity value with color based on threshold
fn format_complexity(complexity: u32, threshold: u32) -> String {
    let s = complexity.to_string();
    if threshold > 0 && complexity >= threshold {
        s.red().bold().to_string()
    } else if complexity > 100 {
        s.yellow().to_string()
    } else {
        s.green().to_string()
    }
}
