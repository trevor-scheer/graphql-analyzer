use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::commands::fix::{apply_fixes, collect_fixable_diagnostics, display_dry_run};
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_ide::DiagnosticSeverity;
use std::path::PathBuf;
use std::process;

#[allow(clippy::too_many_lines)]
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    _watch: bool,
    fix: bool,
    fix_dry_run: bool,
) -> Result<()> {
    // Define diagnostic output structure for collecting warnings and errors
    struct DiagnosticOutput {
        file_path: String,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        message: String,
        severity: String,
        rule: Option<String>,
    }

    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "lint")?;

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

    // Handle fix modes
    let mut fixes_applied = 0;
    let host = if fix || fix_dry_run {
        let spinner = if matches!(format, OutputFormat::Human) {
            Some(crate::progress::spinner("Collecting fixable issues..."))
        } else {
            None
        };

        let fixes = collect_fixable_diagnostics(&host, None);
        fixes_applied = fixes.iter().map(|f| f.diagnostics.len()).sum();

        if let Some(pb) = spinner {
            pb.finish_and_clear();
        }

        if fixes_applied > 0 {
            if fix_dry_run {
                display_dry_run(&fixes, format);
                host
            } else {
                apply_fixes(&fixes, format)?;
                // Reload host to pick up fixed files
                CliAnalysisHost::from_project_config(&project_config, &ctx.base_dir)?
            }
        } else {
            host
        }
    } else {
        host
    };

    // Run lints
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Running lint rules..."))
    } else {
        None
    };

    let lint_start = std::time::Instant::now();
    let all_diagnostics = host.all_lint_diagnostics();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let lint_duration = lint_start.elapsed();

    // Convert diagnostics to CLI output format
    let mut all_warnings = Vec::new();
    let mut all_errors = Vec::new();

    for (file_path, diagnostics) in all_diagnostics {
        for diag in diagnostics {
            let severity_string = match diag.severity {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Information => "info",
                DiagnosticSeverity::Hint => "hint",
            }
            .to_string();

            let diag_output = DiagnosticOutput {
                file_path: file_path.to_string_lossy().into(),
                // Convert from 0-based to 1-based for display
                line: (diag.range.start.line + 1) as usize,
                column: (diag.range.start.character + 1) as usize,
                end_line: (diag.range.end.line + 1) as usize,
                end_column: (diag.range.end.character + 1) as usize,
                message: diag.message,
                severity: severity_string,
                rule: diag.code,
            };

            match diag.severity {
                DiagnosticSeverity::Warning
                | DiagnosticSeverity::Information
                | DiagnosticSeverity::Hint => {
                    all_warnings.push(diag_output);
                }
                DiagnosticSeverity::Error => all_errors.push(diag_output),
            }
        }
    }

    // Display results
    let total_warnings = all_warnings.len();
    let total_errors = all_errors.len();

    match format {
        OutputFormat::Human => {
            // Print all warnings
            for warning in &all_warnings {
                println!(
                    "\n{}:{}:{}: {} {}",
                    warning.file_path,
                    warning.line,
                    warning.column,
                    "warning:".yellow().bold(),
                    warning.message.yellow()
                );
                if let Some(ref rule) = warning.rule {
                    println!("  {}: {}", "rule".dimmed(), rule.dimmed());
                }
            }

            // Print all errors
            for error in &all_errors {
                println!(
                    "\n{}:{}:{}: {} {}",
                    error.file_path,
                    error.line,
                    error.column,
                    "error:".red().bold(),
                    error.message.red()
                );
                if let Some(ref rule) = error.rule {
                    println!("  {}: {}", "rule".dimmed(), rule.dimmed());
                }
            }
        }
        OutputFormat::Json => {
            // Print all diagnostics as JSON
            for warning in &all_warnings {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": warning.file_path,
                        "severity": warning.severity,
                        "rule": warning.rule,
                        "message": warning.message,
                        "location": {
                            "start": {
                                "line": warning.line,
                                "column": warning.column
                            },
                            "end": {
                                "line": warning.end_line,
                                "column": warning.end_column
                            }
                        }
                    })
                );
            }

            for error in &all_errors {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": error.file_path,
                        "severity": error.severity,
                        "rule": error.rule,
                        "message": error.message,
                        "location": {
                            "start": {
                                "line": error.line,
                                "column": error.column
                            },
                            "end": {
                                "line": error.end_line,
                                "column": error.end_column
                            }
                        }
                    })
                );
            }
        }
    }

    // Summary
    let total_duration = start_time.elapsed();
    if matches!(format, OutputFormat::Human) {
        println!();

        // Report fixes if any were applied/detected
        if fixes_applied > 0 && fix {
            println!(
                "{}",
                format!("✓ Fixed {fixes_applied} issue(s)").green().bold()
            );
        } else if fixes_applied > 0 && fix_dry_run {
            println!(
                "{}",
                format!("ℹ Would fix {fixes_applied} issue(s)").cyan().bold()
            );
        }

        if total_errors == 0 && total_warnings == 0 {
            println!("{}", "✓ No linting issues found!".green().bold());
        } else if total_errors == 0 {
            println!(
                "{}",
                format!("✓ Linting passed with {total_warnings} warning(s)")
                    .yellow()
                    .bold()
            );
        } else if total_warnings == 0 {
            println!("{}", format!("✗ Found {total_errors} error(s)").red());
        } else {
            println!(
                "{}",
                format!("✗ Found {total_errors} error(s) and {total_warnings} warning(s)").red()
            );
        }
        println!(
            "  {} load: {:.2}s, linting: {:.2}s, total: {:.2}s",
            "⏱".dimmed(),
            load_duration.as_secs_f64(),
            lint_duration.as_secs_f64(),
            total_duration.as_secs_f64()
        );
    }

    if total_errors > 0 {
        process::exit(1);
    }

    Ok(())
}
