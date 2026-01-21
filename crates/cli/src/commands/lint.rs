use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::commands::fix::{apply_fixes, collect_fixable_diagnostics, display_dry_run};
use crate::watch::{FileWatcher, WatchConfig, WatchMode};
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_ide::DiagnosticSeverity;
use std::path::PathBuf;
use std::process;

/// Diagnostic output structure for collecting warnings and errors
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

/// File-level diagnostic grouping for JSON output
struct FileDiagnostics {
    errors: Vec<DiagnosticOutput>,
    warnings: Vec<DiagnosticOutput>,
}

pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    watch: bool,
    fix: bool,
    fix_dry_run: bool,
) -> Result<()> {
    if watch {
        if fix || fix_dry_run {
            eprintln!(
                "{}",
                "Warning: --fix and --fix-dry-run are ignored in watch mode".yellow()
            );
        }
        return run_watch_mode(config_path, project_name, format);
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

    // Convert diagnostics to CLI output format, grouped by file
    let mut files_with_diagnostics: std::collections::HashMap<String, FileDiagnostics> =
        std::collections::HashMap::new();

    for (file_path, diagnostics) in all_diagnostics {
        let file_path_str = file_path.to_string_lossy().to_string();

        for diag in diagnostics {
            let severity_string = match diag.severity {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Information => "info",
                DiagnosticSeverity::Hint => "hint",
            }
            .to_string();

            let diag_output = DiagnosticOutput {
                file_path: file_path_str.clone(),
                // Convert from 0-based to 1-based for display
                line: (diag.range.start.line + 1) as usize,
                column: (diag.range.start.character + 1) as usize,
                end_line: (diag.range.end.line + 1) as usize,
                end_column: (diag.range.end.character + 1) as usize,
                message: diag.message,
                severity: severity_string,
                rule: diag.code,
            };

            let file_diags = files_with_diagnostics
                .entry(file_path_str.clone())
                .or_insert_with(|| FileDiagnostics {
                    errors: Vec::new(),
                    warnings: Vec::new(),
                });

            match diag.severity {
                DiagnosticSeverity::Warning
                | DiagnosticSeverity::Information
                | DiagnosticSeverity::Hint => {
                    file_diags.warnings.push(diag_output);
                }
                DiagnosticSeverity::Error => file_diags.errors.push(diag_output),
            }
        }
    }

    // Flatten for counting and human output
    let all_warnings: Vec<_> = files_with_diagnostics
        .values()
        .flat_map(|f| &f.warnings)
        .collect();
    let all_errors: Vec<_> = files_with_diagnostics
        .values()
        .flat_map(|f| &f.errors)
        .collect();
    let total_warnings = all_warnings.len();
    let total_errors = all_errors.len();
    let total_files = files_with_diagnostics.len();

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
            // Build aggregated JSON output
            let diag_to_json = |d: &DiagnosticOutput| {
                serde_json::json!({
                    "message": d.message,
                    "severity": d.severity,
                    "rule": d.rule,
                    "location": {
                        "start": { "line": d.line, "column": d.column },
                        "end": { "line": d.end_line, "column": d.end_column }
                    }
                })
            };

            let mut files: Vec<serde_json::Value> = files_with_diagnostics
                .iter()
                .map(|(file, diags)| {
                    serde_json::json!({
                        "file": file,
                        "errors": diags.errors.iter().map(diag_to_json).collect::<Vec<_>>(),
                        "warnings": diags.warnings.iter().map(diag_to_json).collect::<Vec<_>>()
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
                "files": files,
                "stats": {
                    "total_files": total_files,
                    "total_errors": total_errors,
                    "total_warnings": total_warnings
                }
            });

            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Github => {
            // Print GitHub Actions workflow commands
            for warning in &all_warnings {
                let rule_suffix = warning
                    .rule
                    .as_ref()
                    .map(|r| format!(" [{r}]"))
                    .unwrap_or_default();
                println!(
                    "::warning file={},line={},col={}::{}{}",
                    warning.file_path, warning.line, warning.column, warning.message, rule_suffix
                );
            }

            for error in &all_errors {
                let rule_suffix = error
                    .rule
                    .as_ref()
                    .map(|r| format!(" [{r}]"))
                    .unwrap_or_default();
                println!(
                    "::error file={},line={},col={}::{}{}",
                    error.file_path, error.line, error.column, error.message, rule_suffix
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
                format!("ℹ Would fix {fixes_applied} issue(s)")
                    .cyan()
                    .bold()
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

/// Run lint in watch mode
fn run_watch_mode(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Load config
    let ctx = CommandContext::load(config_path, project_name, "lint")?;

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
        mode: WatchMode::Lint,
        format,
        project_config,
        base_dir: ctx.base_dir,
    };

    // Create and run watcher
    let mut watcher = FileWatcher::new(watch_config)?;
    watcher.start()?;
    watcher.run()
}
