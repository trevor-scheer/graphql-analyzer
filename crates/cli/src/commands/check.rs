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
use crate::watch::{FileWatcher, WatchConfig, WatchMode};
use crate::{ExitCode, OutputFormat, OutputOptions};
use anyhow::Result;
use colored::Colorize;
use graphql_ide::DiagnosticSeverity;
use std::path::PathBuf;

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
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    watch: bool,
    output_opts: OutputOptions,
) -> Result<()> {
    if watch {
        return run_watch_mode(config_path, project_name, format);
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
    let spinner = if matches!(format, OutputFormat::Human) && output_opts.show_progress {
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
            ExitCode::SchemaError.exit();
        })
        .unwrap();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let load_duration = load_start.elapsed();

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) && output_opts.show_info {
        if host.schema_loaded() {
            println!("{}", "✓ Schema loaded successfully".green());
        } else {
            println!(
                "{}",
                "! No schema files found matching configured patterns. Schema validation will be skipped.".yellow()
            );
        }
        println!("{}", "✓ Documents loaded successfully".green());
    }

    // Collect all diagnostics
    let mut all_issues: Vec<DiagnosticOutput> = Vec::new();

    // Run validation
    let spinner = if matches!(format, OutputFormat::Human) && output_opts.show_progress {
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

    // Group issues by file for JSON output
    let mut files_with_issues: std::collections::HashMap<
        String,
        (Vec<&DiagnosticOutput>, Vec<&DiagnosticOutput>),
    > = std::collections::HashMap::new();

    for issue in &all_issues {
        let (errors, warnings) = files_with_issues
            .entry(issue.file_path.clone())
            .or_insert_with(|| (Vec::new(), Vec::new()));
        if issue.severity == "error" {
            errors.push(issue);
        } else {
            warnings.push(issue);
        }
    }

    let total_files = files_with_issues.len();

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
            // Build aggregated JSON output
            let issue_to_json = |issue: &&DiagnosticOutput| {
                let mut obj = serde_json::json!({
                    "message": issue.message,
                    "severity": issue.severity,
                    "source": issue.source.to_string(),
                    "rule": issue.rule
                });
                if issue.line > 0 {
                    obj["location"] = serde_json::json!({
                        "start": { "line": issue.line, "column": issue.column },
                        "end": { "line": issue.end_line, "column": issue.end_column }
                    });
                }
                obj
            };

            let mut files: Vec<serde_json::Value> = files_with_issues
                .iter()
                .map(|(file, (errors, warnings))| {
                    serde_json::json!({
                        "file": file,
                        "errors": errors.iter().map(issue_to_json).collect::<Vec<_>>(),
                        "warnings": warnings.iter().map(issue_to_json).collect::<Vec<_>>()
                    })
                })
                .collect();

            // Sort files for consistent output
            files.sort_by(|a, b| {
                a.get("file")
                    .and_then(|v| v.as_str())
                    .cmp(&b.get("file").and_then(|v| v.as_str()))
            });

            let output = serde_json::json!({
                "success": total_errors == 0,
                "schema_loaded": host.schema_loaded(),
                "files": files,
                "stats": {
                    "total_files": total_files,
                    "total_errors": total_errors,
                    "total_warnings": total_warnings,
                    "validation_errors": validation_errors,
                    "lint_errors": lint_errors
                }
            });

            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Github => {
            // Print GitHub Actions workflow commands
            for issue in &all_issues {
                let severity = match issue.severity.as_str() {
                    "error" => "error",
                    "warning" => "warning",
                    _ => "notice",
                };
                let rule_suffix = issue
                    .rule
                    .as_ref()
                    .map(|r| format!(" [{r}]"))
                    .unwrap_or_default();

                if issue.line > 0 {
                    println!(
                        "::{} file={},line={},col={}::{}{}",
                        severity,
                        issue.file_path,
                        issue.line,
                        issue.column,
                        issue.message,
                        rule_suffix
                    );
                } else {
                    println!("::{}::{}{}", severity, issue.message, rule_suffix);
                }
            }
        }
    }

    // Summary
    let total_duration = start_time.elapsed();
    if matches!(format, OutputFormat::Human) && output_opts.show_info {
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
        ExitCode::ValidationError.exit();
    }

    Ok(())
}

/// Run check in watch mode
fn run_watch_mode(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Load config
    let ctx = CommandContext::load(config_path, project_name, "check")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Create watch config
    let watch_config = WatchConfig {
        mode: WatchMode::Check,
        format,
        project_config,
        base_dir: ctx.base_dir,
    };

    // Create and run watcher
    let mut watcher = FileWatcher::new(watch_config)?;
    watcher.start()?;
    watcher.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_source_display_validation() {
        let source = DiagnosticSource::Validation;
        assert_eq!(format!("{source}"), "validation");
    }

    #[test]
    fn test_diagnostic_source_display_lint() {
        let source = DiagnosticSource::Lint;
        assert_eq!(format!("{source}"), "lint");
    }

    #[test]
    fn test_diagnostic_source_debug() {
        let source = DiagnosticSource::Validation;
        assert_eq!(format!("{source:?}"), "Validation");
    }

    #[test]
    fn test_diagnostic_source_clone() {
        let source = DiagnosticSource::Lint;
        let cloned = source;
        assert!(matches!(cloned, DiagnosticSource::Lint));
    }

    #[test]
    fn test_diagnostic_source_copy() {
        let source = DiagnosticSource::Validation;
        let copied = source;
        // Both should be valid since DiagnosticSource is Copy
        assert!(matches!(source, DiagnosticSource::Validation));
        assert!(matches!(copied, DiagnosticSource::Validation));
    }
}
