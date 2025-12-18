use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::process;
use tracing::Instrument;

#[allow(clippy::too_many_lines)]
#[tracing::instrument(skip(config_path, project_name, format), fields(project = ?project_name))]
pub async fn run(
    config_path: Option<PathBuf>,
    project_name: Option<String>,
    format: OutputFormat,
    watch: bool,
) -> Result<()> {
    // Define diagnostic output structure for collecting errors
    struct DiagnosticOutput {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
    }

    if watch {
        println!("{}", "Watch mode not yet implemented".yellow());
        return Ok(());
    }

    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name.as_ref(), "validate")?;

    // Load and select project
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let load_start = std::time::Instant::now();
    let load_projects_span = tracing::info_span!("load_projects");
    let (project_name, project) = async {
        ctx.load_project(project_name.as_deref())
            .await
            .map_err(|e| {
                if matches!(format, OutputFormat::Human) {
                    eprintln!("{} {}", "✗ Failed to load project:".red(), e);
                } else {
                    eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
                }
                process::exit(1);
            })
            .unwrap()
    }
    .instrument(load_projects_span)
    .await;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let load_duration = load_start.elapsed();

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) {
        CommandContext::print_success_message(&project);
    }

    // Validate all files
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Validating GraphQL documents..."))
    } else {
        None
    };

    let validate_start = std::time::Instant::now();
    let validate_span = tracing::info_span!("validate_all", project = %project_name);
    let all_diagnostics = async { project.validate_all() }
        .instrument(validate_span)
        .await;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let validate_duration = validate_start.elapsed();

    tracing::info!(
        files_with_diagnostics = all_diagnostics.len(),
        "Validation completed"
    );

    // Convert diagnostics to CLI output format
    let mut all_errors = Vec::new();
    for (file_path, diagnostics) in all_diagnostics {
        for diag in diagnostics {
            use graphql_project::Severity;

            // Only process errors (Apollo compiler validation)
            if diag.severity == Severity::Error {
                let diag_output = DiagnosticOutput {
                    file_path: file_path.to_string_lossy().to_string(),
                    // graphql-project uses 0-based, CLI output uses 1-based
                    line: diag.range.start.line + 1,
                    column: diag.range.start.character + 1,
                    message: diag.message,
                };

                all_errors.push(diag_output);
            }
        }
    }

    // Display errors
    let total_errors = all_errors.len();

    match format {
        OutputFormat::Human => {
            // Print all errors
            for error in &all_errors {
                if error.line > 0 {
                    println!(
                        "\n{}:{}:{}: {} {}",
                        error.file_path,
                        error.line,
                        error.column,
                        "error:".red().bold(),
                        error.message.red()
                    );
                } else {
                    // No location info
                    println!("\n{}", error.message.red());
                }
            }
        }
        OutputFormat::Json => {
            // Print all errors as JSON
            for error in &all_errors {
                let location = if error.line > 0 {
                    Some(serde_json::json!({
                        "line": error.line,
                        "column": error.column
                    }))
                } else {
                    None
                };

                println!(
                    "{}",
                    serde_json::json!({
                        "file": error.file_path,
                        "severity": "error",
                        "message": error.message,
                        "location": location
                    })
                );
            }
        }
    }

    // Summary
    let total_duration = start_time.elapsed();
    if matches!(format, OutputFormat::Human) {
        println!();
        if total_errors == 0 {
            println!("{}", "✓ All validations passed!".green().bold());
        } else {
            println!("{}", format!("✗ Found {total_errors} error(s)").red());
        }
        println!(
            "  {} load: {:.2}s, validation: {:.2}s, total: {:.2}s",
            "⏱".dimmed(),
            load_duration.as_secs_f64(),
            validate_duration.as_secs_f64(),
            total_duration.as_secs_f64()
        );
    }

    if total_errors > 0 {
        process::exit(1);
    }

    Ok(())
}
