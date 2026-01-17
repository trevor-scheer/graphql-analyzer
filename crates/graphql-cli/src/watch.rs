//! File watching support for CLI commands.
//!
//! Provides continuous validation/linting as files change.

use crate::analysis::CliAnalysisHost;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_config::ProjectConfig;
use graphql_ide::DiagnosticSeverity;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Type of diagnostics to run in watch mode.
#[derive(Debug, Clone, Copy)]
pub enum WatchMode {
    /// Only run GraphQL spec validation.
    Validate,
    /// Only run lint rules.
    Lint,
    /// Run both validation and linting.
    Check,
}

/// Configuration for watch mode.
pub struct WatchConfig<'a> {
    pub mode: WatchMode,
    pub format: OutputFormat,
    pub project_config: &'a ProjectConfig,
    pub base_dir: &'a Path,
}

/// Diagnostic output structure for display.
pub struct DiagnosticOutput {
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub message: String,
    pub severity: String,
    pub source: &'static str,
    pub rule: Option<String>,
}

/// Run watch mode with the given configuration.
pub fn run_watch(config: WatchConfig) -> Result<()> {
    let WatchConfig {
        mode,
        format,
        project_config,
        base_dir,
    } = config;

    // Initial load
    if matches!(format, OutputFormat::Human) {
        println!("{}", "Starting watch mode...".cyan().bold());
        println!();
    }

    let mut host = CliAnalysisHost::from_project_config(project_config, base_dir)?;

    if matches!(format, OutputFormat::Human) {
        println!("{}", "✓ Project loaded successfully".green());
    }

    // Run initial check
    run_diagnostics(&host, mode, format);

    // Determine watch paths from the project config
    let watch_paths = get_watch_paths(project_config, base_dir);

    if matches!(format, OutputFormat::Human) {
        println!();
        println!(
            "{} {}",
            "Watching for changes in".dimmed(),
            format!("{} path(s)...", watch_paths.len()).dimmed()
        );
        println!("{}", "Press Ctrl+C to stop.".dimmed());
        println!();
    }

    // Set up file watcher with debouncing
    let (tx, rx) = mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_millis(200), move |res: DebounceEventResult| {
        if let Ok(events) = res {
            let _ = tx.send(events);
        }
    })?;

    // Watch all relevant directories
    for path in &watch_paths {
        if path.exists() {
            debouncer.watcher().watch(path, RecursiveMode::Recursive)?;
        }
    }

    // Main watch loop
    loop {
        match rx.recv() {
            Ok(events) => {
                // Collect changed files
                let changed_files: HashSet<PathBuf> = events
                    .into_iter()
                    .filter_map(|event| {
                        let path = event.path;
                        if should_process_file(&path, project_config) {
                            Some(path)
                        } else {
                            None
                        }
                    })
                    .collect();

                if changed_files.is_empty() {
                    continue;
                }

                // Update changed files
                for path in &changed_files {
                    if path.exists() {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            host.update_file(path, &content);
                        }
                    } else {
                        // File was deleted - we could remove it, but for now just skip
                        continue;
                    }
                }

                // Clear screen for human output
                if matches!(format, OutputFormat::Human) {
                    print!("\x1B[2J\x1B[1;1H"); // Clear screen and move to top
                    let files_list: Vec<_> = changed_files
                        .iter()
                        .filter_map(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .collect();
                    println!(
                        "{} {}",
                        "Change detected:".cyan(),
                        files_list.join(", ").dimmed()
                    );
                    println!();
                }

                // Re-run diagnostics
                run_diagnostics(&host, mode, format);

                if matches!(format, OutputFormat::Human) {
                    println!();
                    println!("{}", "Watching for changes...".dimmed());
                }
            }
            Err(_) => {
                // Channel closed, exit
                break;
            }
        }
    }

    Ok(())
}

/// Run diagnostics based on the watch mode.
fn run_diagnostics(host: &CliAnalysisHost, mode: WatchMode, format: OutputFormat) {
    let mut all_issues: Vec<DiagnosticOutput> = Vec::new();

    // Collect validation diagnostics
    if matches!(mode, WatchMode::Validate | WatchMode::Check) {
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
                        source: "validation",
                        rule: diag.code,
                    });
                }
            }
        }
    }

    // Collect lint diagnostics
    if matches!(mode, WatchMode::Lint | WatchMode::Check) {
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
                    source: "lint",
                    rule: diag.code,
                });
            }
        }
    }

    // Sort issues
    all_issues.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.column.cmp(&b.column))
    });

    // Count issues
    let total_errors = all_issues.iter().filter(|i| i.severity == "error").count();
    let total_warnings = all_issues
        .iter()
        .filter(|i| i.severity == "warning")
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
                        "{}:{}:{}: {} {}",
                        issue.file_path, issue.line, issue.column, severity_styled, message_styled
                    );
                } else {
                    println!("{severity_styled} {message_styled}");
                }

                if let Some(ref rule) = issue.rule {
                    println!("  {}: {}", "rule".dimmed(), rule.dimmed());
                }
            }

            // Summary
            println!();
            if total_errors == 0 && total_warnings == 0 {
                println!("{}", "✓ All checks passed!".green().bold());
            } else if total_errors == 0 {
                println!(
                    "{}",
                    format!("⚠ {total_warnings} warning(s)").yellow().bold()
                );
            } else {
                println!(
                    "{}",
                    format!("✗ {total_errors} error(s), {total_warnings} warning(s)")
                        .red()
                        .bold()
                );
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
                        "source": issue.source,
                        "rule": issue.rule,
                        "message": issue.message,
                        "location": location
                    })
                );
            }
        }
    }
}

/// Get paths to watch based on project configuration.
fn get_watch_paths(project_config: &ProjectConfig, base_dir: &Path) -> Vec<PathBuf> {
    let mut paths = HashSet::new();

    // Add schema paths
    for schema_source in &project_config.schema {
        if let graphql_config::SchemaSource::File(pattern) = schema_source {
            // Get the directory part of the glob pattern
            let pattern_path = base_dir.join(pattern);
            if let Some(parent) = get_glob_base_dir(&pattern_path) {
                paths.insert(parent);
            }
        }
    }

    // Add document paths
    for doc_glob in &project_config.documents {
        let pattern_path = base_dir.join(doc_glob);
        if let Some(parent) = get_glob_base_dir(&pattern_path) {
            paths.insert(parent);
        }
    }

    // If no paths found, watch the base directory
    if paths.is_empty() {
        paths.insert(base_dir.to_path_buf());
    }

    paths.into_iter().collect()
}

/// Get the base directory for a glob pattern (the non-glob prefix).
fn get_glob_base_dir(pattern: &Path) -> Option<PathBuf> {
    let pattern_str = pattern.to_string_lossy();

    // Find the first component with glob characters
    let mut base = PathBuf::new();
    for component in pattern.components() {
        let comp_str = component.as_os_str().to_string_lossy();
        if comp_str.contains('*') || comp_str.contains('?') || comp_str.contains('[') {
            break;
        }
        base.push(component);
    }

    // If we have a non-empty base that exists, use it
    if base.as_os_str().is_empty() {
        // Pattern starts with glob, use parent directory
        if let Some(parent) = pattern.parent() {
            return Some(parent.to_path_buf());
        }
        return None;
    }

    // If base is a file, use its parent
    if base.is_file() {
        return base.parent().map(|p| p.to_path_buf());
    }

    // If base exists as a directory, use it
    if base.exists() {
        return Some(base);
    }

    // Otherwise try parent
    base.parent().map(|p| p.to_path_buf())
}

/// Check if a file should be processed based on extension.
fn should_process_file(path: &Path, _project_config: &ProjectConfig) -> bool {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    matches!(
        extension,
        "graphql" | "gql" | "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" | "cts" | "cjs"
    )
}
