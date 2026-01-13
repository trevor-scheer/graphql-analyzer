//! Combined validate + lint command for CI pipelines.
//!
//! The `check` command runs both GraphQL spec validation and custom lint rules
//! in a single pass, providing unified output and exit codes. This is the
//! recommended command for CI/CD pipelines.
//!
//! Equivalent to running `graphql validate && graphql lint` but more efficient
//! because it only loads the project once.

use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_ide::DiagnosticSeverity;
use std::path::PathBuf;
use std::process;

/// Diagnostic output structure for unified display
struct DiagnosticOutput {
    file_path: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    message: String,
    severity: String,
    source: DiagnosticSource,
    rule: Option<String>,
}

/// Source of the diagnostic (validation or lint)
#[derive(Debug, Clone, Copy)]
enum DiagnosticSource {
    Validation,
    Lint,
}

impl std::fmt::Display for DiagnosticSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation => write!(f, "validation"),
            Self::Lint => write!(f, "lint"),
        }
    }
}

/// Run the check command (combined validate + lint).
///
/// This command:
/// 1. Loads the project configuration and files
/// 2. Runs GraphQL spec validation
/// 3. Runs custom lint rules
/// 4. Reports all issues with unified output
/// 5. Returns appropriate exit code (1 if any errors)
#[allow(clippy::too_many_lines)]
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    watch: bool,
) -> Result<()> {
    if watch {
        println!("{}", "Watch mode not yet implemented".yellow());
        return Ok(());
    }

    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "check")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Load and select project (shared between validate and lint)
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let load_start = std::time::Instant::now();
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

    let load_duration = load_start.elapsed();

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) {
        println!("{}", "✓ Schema loaded successfully".green());
        println!("{}", "✓ Documents loaded successfully".green());
    }

    // Collect all diagnostics
    let mut all_issues: Vec<DiagnosticOutput> = Vec::new();

    // Run validation
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner(
            "Running validation and lint checks...",
        ))
    } else {
        None
    };

    let validate_start = std::time::Instant::now();
    let validation_diagnostics = host.all_validation_diagnostics();

    for (file_path, diagnostics) in validation_diagnostics {
        for diag in diagnostics {
            if diag.severity == DiagnosticSeverity::Error {
                all_issues.push(DiagnosticOutput {
                    file_path: file_path.to_string_lossy().to_string(),
                    line: (diag.range.start.line + 1) as usize,
                    column: (diag.range.start.character + 1) as usize,
                    end_line: (diag.range.end.line + 1) as usize,
                    end_column: (diag.range.end.character + 1) as usize,
                    message: diag.message,
                    severity: "error".to_string(),
                    source: DiagnosticSource::Validation,
                    rule: diag.code,
                });
            }
        }
    }

    let validate_duration = validate_start.elapsed();

    // Run linting
    let lint_start = std::time::Instant::now();
    let lint_diagnostics = host.all_lint_diagnostics();

    for (file_path, diagnostics) in lint_diagnostics {
        for diag in diagnostics {
            let severity_string = match diag.severity {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Information => "info",
                DiagnosticSeverity::Hint => "hint",
            }
            .to_string();

            all_issues.push(DiagnosticOutput {
                file_path: file_path.to_string_lossy().to_string(),
                line: (diag.range.start.line + 1) as usize,
                column: (diag.range.start.character + 1) as usize,
                end_line: (diag.range.end.line + 1) as usize,
                end_column: (diag.range.end.character + 1) as usize,
                message: diag.message,
                severity: severity_string,
                source: DiagnosticSource::Lint,
                rule: diag.code,
            });
        }
    }

    let lint_duration = lint_start.elapsed();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Sort issues by file path, then by line number for consistent output
    all_issues.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.column.cmp(&b.column))
    });

    // Count errors and warnings
    let total_errors = all_issues.iter().filter(|i| i.severity == "error").count();
    let total_warnings = all_issues
        .iter()
        .filter(|i| i.severity == "warning")
        .count();
    let validation_errors = all_issues
        .iter()
        .filter(|i| matches!(i.source, DiagnosticSource::Validation) && i.severity == "error")
        .count();
    let lint_errors = all_issues
        .iter()
        .filter(|i| matches!(i.source, DiagnosticSource::Lint) && i.severity == "error")
        .count();

    // Display results
    match format {
        OutputFormat::Human => {
            for issue in &all_issues {
                let severity_styled = match issue.severity.as_str() {
                    "error" => "error:".red().bold(),
                    "warning" => "warning:".yellow().bold(),
                    _ => "info:".dimmed(),
                };

                let message_styled = match issue.severity.as_str() {
                    "error" => issue.message.red(),
                    "warning" => issue.message.yellow(),
                    _ => issue.message.normal(),
                };

                if issue.line > 0 {
                    println!(
                        "\n{}:{}:{}: {} {}",
                        issue.file_path, issue.line, issue.column, severity_styled, message_styled
                    );
                } else {
                    println!("\n{severity_styled} {message_styled}");
                }

                if let Some(ref rule) = issue.rule {
                    println!("  {}: {}", "rule".dimmed(), rule.dimmed());
                }
            }
        }
        OutputFormat::Json => {
            for issue in &all_issues {
                let location = if issue.line > 0 {
                    Some(serde_json::json!({
                        "start": {
                            "line": issue.line,
                            "column": issue.column
                        },
                        "end": {
                            "line": issue.end_line,
                            "column": issue.end_column
                        }
                    }))
                } else {
                    None
                };

                println!(
                    "{}",
                    serde_json::json!({
                        "file": issue.file_path,
                        "severity": issue.severity,
                        "source": issue.source.to_string(),
                        "rule": issue.rule,
                        "message": issue.message,
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
        if total_errors == 0 && total_warnings == 0 {
            println!("{}", "✓ All checks passed!".green().bold());
        } else if total_errors == 0 {
            println!(
                "{}",
                format!("✓ Checks passed with {total_warnings} warning(s)")
                    .yellow()
                    .bold()
            );
        } else {
            let mut parts = Vec::new();
            if validation_errors > 0 {
                parts.push(format!("{validation_errors} validation error(s)"));
            }
            if lint_errors > 0 {
                parts.push(format!("{lint_errors} lint error(s)"));
            }
            if total_warnings > 0 {
                parts.push(format!("{total_warnings} warning(s)"));
            }
            println!("{}", format!("✗ Found {}", parts.join(", ")).red());
        }
        println!(
            "  {} load: {:.2}s, validation: {:.2}s, linting: {:.2}s, total: {:.2}s",
            "⏱".dimmed(),
            load_duration.as_secs_f64(),
            validate_duration.as_secs_f64(),
            lint_duration.as_secs_f64(),
            total_duration.as_secs_f64()
        );
    }

    if total_errors > 0 {
        process::exit(1);
    }

    Ok(())
}
