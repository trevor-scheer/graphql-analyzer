//! Watch mode infrastructure for CLI commands.
//!
//! This module provides file watching capabilities for validate, lint, and check commands.
//! It uses the `notify` crate for cross-platform file system events and implements
//! debouncing to handle rapid file changes efficiently.

use crate::analysis::CliAnalysisHost;
use crate::OutputFormat;
use anyhow::{Context, Result};
use colored::Colorize;
use graphql_config::ProjectConfig;
use graphql_ide::{Diagnostic, DiagnosticSeverity, FileKind};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// Debounce duration for file changes (milliseconds)
const DEBOUNCE_MS: u64 = 100;

/// What kind of checks to run in watch mode
#[derive(Debug, Clone, Copy)]
pub enum WatchMode {
    /// Only run GraphQL spec validation
    Validate,
    /// Only run custom lint rules
    Lint,
    /// Run both validation and linting
    Check,
}

/// Result of a single check run
pub struct CheckResult {
    pub validation_errors: usize,
    pub lint_errors: usize,
    pub lint_warnings: usize,
    pub changed_files: Vec<PathBuf>,
    pub duration: Duration,
}

/// Watch configuration
pub struct WatchConfig {
    pub mode: WatchMode,
    pub format: OutputFormat,
    pub project_config: ProjectConfig,
    pub base_dir: PathBuf,
}

/// File watcher that runs checks on file changes
pub struct FileWatcher {
    config: WatchConfig,
    host: CliAnalysisHost,
    watcher: RecommendedWatcher,
    rx: Receiver<Result<Event, notify::Error>>,
    watch_paths: HashSet<PathBuf>,
}

impl FileWatcher {
    /// Create a new file watcher
    pub fn new(config: WatchConfig) -> Result<Self> {
        // Create initial analysis host
        let host = CliAnalysisHost::from_project_config(&config.project_config, &config.base_dir)?;

        // Collect paths to watch
        let watch_paths = Self::collect_watch_paths(&config.project_config, &config.base_dir);

        // Set up file watcher
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .context("Failed to create file watcher")?;

        Ok(Self {
            config,
            host,
            watcher,
            rx,
            watch_paths,
        })
    }

    /// Collect all paths that need to be watched
    fn collect_watch_paths(project_config: &ProjectConfig, base_dir: &Path) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();

        // Watch schema files
        match &project_config.schema {
            graphql_config::SchemaConfig::Path(path) => {
                let schema_path = base_dir.join(path);
                if let Some(parent) = schema_path.parent() {
                    paths.insert(parent.to_path_buf());
                }
            }
            graphql_config::SchemaConfig::Paths(schema_paths) => {
                for path in schema_paths {
                    let schema_path = base_dir.join(path);
                    if let Some(parent) = schema_path.parent() {
                        paths.insert(parent.to_path_buf());
                    }
                }
            }
            graphql_config::SchemaConfig::Introspection(_) => {
                // Remote schemas don't need watching
            }
        }

        // Watch document directories
        if let Some(ref docs) = project_config.documents {
            for pattern in docs.patterns() {
                // Extract the base directory from the glob pattern
                let pattern_path = base_dir.join(pattern);
                let pattern_str = pattern_path.to_string_lossy();

                // Find the first directory that doesn't contain glob characters
                let base = pattern_str
                    .split(&['*', '?', '[', ']'][..])
                    .next()
                    .unwrap_or(".");

                let base_path = PathBuf::from(base);
                if base_path.exists() {
                    // Watch the deepest existing directory
                    let mut watch_dir = base_path.clone();
                    while !watch_dir.exists() && watch_dir.parent().is_some() {
                        watch_dir = watch_dir.parent().unwrap().to_path_buf();
                    }
                    if watch_dir.exists() && watch_dir.is_dir() {
                        paths.insert(watch_dir);
                    }
                } else if let Some(parent) = base_path.parent() {
                    if parent.exists() {
                        paths.insert(parent.to_path_buf());
                    }
                }
            }
        }

        // If no specific paths found, watch base_dir
        if paths.is_empty() {
            paths.insert(base_dir.to_path_buf());
        }

        paths
    }

    /// Start watching for file changes
    pub fn start(&mut self) -> Result<()> {
        for path in &self.watch_paths {
            self.watcher
                .watch(path, RecursiveMode::Recursive)
                .with_context(|| format!("Failed to watch path: {}", path.display()))?;
        }
        Ok(())
    }

    /// Run the watch loop
    pub fn run(&mut self) -> Result<()> {
        // Print initial header
        self.print_header();

        // Run initial check
        let result = self.run_checks(&[]);
        self.print_result(&result, true);

        // Track pending changes for debouncing
        let mut pending_changes: HashSet<PathBuf> = HashSet::new();
        let mut last_change_time: Option<Instant> = None;

        loop {
            // Check for file events with timeout
            let timeout = if last_change_time.is_some() {
                Duration::from_millis(DEBOUNCE_MS)
            } else {
                Duration::from_secs(60)
            };

            match self.rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    // Collect changed files
                    for path in event.paths {
                        if Self::is_relevant_file(&path) {
                            pending_changes.insert(path);
                            last_change_time = Some(Instant::now());
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("{} {}", "Watch error:".red(), e);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Check if we should process pending changes
                    if let Some(last_time) = last_change_time {
                        if last_time.elapsed() >= Duration::from_millis(DEBOUNCE_MS)
                            && !pending_changes.is_empty()
                        {
                            let changed: Vec<PathBuf> = pending_changes.drain().collect();
                            last_change_time = None;

                            // Update files in the host
                            self.update_changed_files(&changed)?;

                            // Run checks
                            let result = self.run_checks(&changed);
                            self.print_result(&result, false);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Check if a file is relevant for watching
    fn is_relevant_file(path: &Path) -> bool {
        let extension = path.extension().and_then(|e| e.to_str());
        matches!(
            extension,
            Some("graphql" | "gql" | "ts" | "tsx" | "js" | "jsx")
        )
    }

    /// Update changed files in the analysis host
    fn update_changed_files(&mut self, changed: &[PathBuf]) -> Result<()> {
        for path in changed {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read file: {}", path.display()))?;

                let kind = match path.extension().and_then(|e| e.to_str()) {
                    Some("ts" | "tsx") => FileKind::TypeScript,
                    Some("js" | "jsx") => FileKind::JavaScript,
                    _ => FileKind::ExecutableGraphQL,
                };

                self.host.update_file(path, &content, kind);
            }
        }

        Ok(())
    }

    /// Run checks based on the watch mode
    fn run_checks(&self, changed_files: &[PathBuf]) -> CheckResult {
        let start = Instant::now();
        let mut validation_errors = 0;
        let mut lint_errors = 0;
        let mut lint_warnings = 0;

        match self.config.mode {
            WatchMode::Validate => {
                let diagnostics = self.host.all_validation_diagnostics();
                validation_errors = count_errors(&diagnostics);
                self.print_diagnostics(&diagnostics, "validation");
            }
            WatchMode::Lint => {
                let diagnostics = self.host.all_lint_diagnostics();
                let (errors, warnings) = count_lint_issues(&diagnostics);
                lint_errors = errors;
                lint_warnings = warnings;
                self.print_diagnostics(&diagnostics, "lint");
            }
            WatchMode::Check => {
                let validation_diags = self.host.all_validation_diagnostics();
                validation_errors = count_errors(&validation_diags);
                self.print_diagnostics(&validation_diags, "validation");

                let lint_diags = self.host.all_lint_diagnostics();
                let (errors, warnings) = count_lint_issues(&lint_diags);
                lint_errors = errors;
                lint_warnings = warnings;
                self.print_diagnostics(&lint_diags, "lint");
            }
        }

        CheckResult {
            validation_errors,
            lint_errors,
            lint_warnings,
            changed_files: changed_files.to_vec(),
            duration: start.elapsed(),
        }
    }

    /// Print diagnostics based on output format
    fn print_diagnostics(&self, diagnostics: &HashMap<PathBuf, Vec<Diagnostic>>, source: &str) {
        match self.config.format {
            OutputFormat::Human => {
                for (file_path, diags) in diagnostics {
                    for diag in diags {
                        let severity_styled = match diag.severity {
                            DiagnosticSeverity::Error => "error:".red().bold(),
                            DiagnosticSeverity::Warning => "warning:".yellow().bold(),
                            _ => "info:".dimmed(),
                        };

                        let message_styled = match diag.severity {
                            DiagnosticSeverity::Error => diag.message.red(),
                            DiagnosticSeverity::Warning => diag.message.yellow(),
                            _ => diag.message.normal(),
                        };

                        let line = diag.range.start.line + 1;
                        let column = diag.range.start.character + 1;

                        println!(
                            "{}:{}:{}: {} {}",
                            file_path.display(),
                            line,
                            column,
                            severity_styled,
                            message_styled
                        );

                        if let Some(ref code) = diag.code {
                            println!("  {}: {}", "rule".dimmed(), code.dimmed());
                        }
                    }
                }
            }
            OutputFormat::Json => {
                for (file_path, diags) in diagnostics {
                    for diag in diags {
                        let severity = match diag.severity {
                            DiagnosticSeverity::Error => "error",
                            DiagnosticSeverity::Warning => "warning",
                            DiagnosticSeverity::Information => "info",
                            DiagnosticSeverity::Hint => "hint",
                        };

                        println!(
                            "{}",
                            serde_json::json!({
                                "type": "diagnostic",
                                "file": file_path.display().to_string(),
                                "source": source,
                                "severity": severity,
                                "rule": diag.code,
                                "message": diag.message,
                                "location": {
                                    "start": {
                                        "line": diag.range.start.line + 1,
                                        "column": diag.range.start.character + 1
                                    },
                                    "end": {
                                        "line": diag.range.end.line + 1,
                                        "column": diag.range.end.character + 1
                                    }
                                }
                            })
                        );
                    }
                }
            }
            OutputFormat::Github => {
                for (file_path, diags) in diagnostics {
                    for diag in diags {
                        let level = match diag.severity {
                            DiagnosticSeverity::Error => "error",
                            DiagnosticSeverity::Warning => "warning",
                            _ => "notice",
                        };
                        let line = diag.range.start.line + 1;
                        let col = diag.range.start.character + 1;
                        let rule_suffix = diag
                            .code
                            .as_ref()
                            .map(|r| format!(" [{r}]"))
                            .unwrap_or_default();
                        println!(
                            "::{level} file={},line={line},col={col}::{}{rule_suffix}",
                            file_path.display(),
                            diag.message
                        );
                    }
                }
            }
        }
    }

    /// Print the watch mode header
    fn print_header(&self) {
        match self.config.format {
            OutputFormat::Human => {
                let mode_name = match self.config.mode {
                    WatchMode::Validate => "validation",
                    WatchMode::Lint => "linting",
                    WatchMode::Check => "checks",
                };
                println!();
                println!(
                    "{} Watching for changes... (press {} to stop)",
                    "●".cyan(),
                    "Ctrl+C".bold()
                );
                println!("  Running {} on file changes", mode_name.cyan());
                println!();
            }
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "start",
                        "mode": format!("{:?}", self.config.mode).to_lowercase(),
                        "timestamp": chrono_now()
                    })
                );
            }
            OutputFormat::Github => {
                // GitHub Actions format uses human-readable header
                let mode_name = match self.config.mode {
                    WatchMode::Validate => "validation",
                    WatchMode::Lint => "linting",
                    WatchMode::Check => "checks",
                };
                println!();
                println!(
                    "{} Watching for changes... (press {} to stop)",
                    "●".cyan(),
                    "Ctrl+C".bold()
                );
                println!("  Running {} on file changes", mode_name.cyan());
                println!();
            }
        }
    }

    /// Print the result of a check run
    fn print_result(&self, result: &CheckResult, is_initial: bool) {
        let total_errors = result.validation_errors + result.lint_errors;

        match self.config.format {
            OutputFormat::Human => {
                let timestamp = format!("[{}]", chrono_now()).dimmed();

                if !is_initial && !result.changed_files.is_empty() {
                    println!();
                    for file in &result.changed_files {
                        println!(
                            "{} {} changed",
                            timestamp,
                            file.file_name()
                                .map_or_else(
                                    || file.display().to_string(),
                                    |n| n.to_string_lossy().to_string(),
                                )
                                .cyan()
                        );
                    }
                }

                println!();
                if total_errors == 0 && result.lint_warnings == 0 {
                    println!("{} {}", timestamp, "✓ All checks passed!".green().bold());
                } else if total_errors == 0 {
                    println!(
                        "{} {}",
                        timestamp,
                        format!("✓ Passed with {} warning(s)", result.lint_warnings)
                            .yellow()
                            .bold()
                    );
                } else {
                    let mut parts = Vec::new();
                    if result.validation_errors > 0 {
                        parts.push(format!("{} validation error(s)", result.validation_errors));
                    }
                    if result.lint_errors > 0 {
                        parts.push(format!("{} lint error(s)", result.lint_errors));
                    }
                    if result.lint_warnings > 0 {
                        parts.push(format!("{} warning(s)", result.lint_warnings));
                    }
                    println!("{} {}", timestamp, format!("✗ {}", parts.join(", ")).red());
                }

                println!("  {} {:.2}s", "⏱".dimmed(), result.duration.as_secs_f64());
            }
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "result",
                        "timestamp": chrono_now(),
                        "initial": is_initial,
                        "changed_files": result.changed_files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                        "validation_errors": result.validation_errors,
                        "lint_errors": result.lint_errors,
                        "lint_warnings": result.lint_warnings,
                        "passed": total_errors == 0,
                        "duration_ms": result.duration.as_millis()
                    })
                );
            }
            OutputFormat::Github => {
                // GitHub Actions format uses human-readable result summary
                let timestamp = format!("[{}]", chrono_now()).dimmed();

                if !is_initial && !result.changed_files.is_empty() {
                    println!();
                    for file in &result.changed_files {
                        println!(
                            "{} {} changed",
                            timestamp,
                            file.file_name()
                                .map_or_else(
                                    || file.display().to_string(),
                                    |n| n.to_string_lossy().to_string(),
                                )
                                .cyan()
                        );
                    }
                }

                println!();
                if total_errors == 0 && result.lint_warnings == 0 {
                    println!("{} {}", timestamp, "✓ All checks passed!".green().bold());
                } else if total_errors == 0 {
                    println!(
                        "{} {}",
                        timestamp,
                        format!("✓ Passed with {} warning(s)", result.lint_warnings)
                            .yellow()
                            .bold()
                    );
                } else {
                    let mut parts = Vec::new();
                    if result.validation_errors > 0 {
                        parts.push(format!("{} validation error(s)", result.validation_errors));
                    }
                    if result.lint_errors > 0 {
                        parts.push(format!("{} lint error(s)", result.lint_errors));
                    }
                    if result.lint_warnings > 0 {
                        parts.push(format!("{} warning(s)", result.lint_warnings));
                    }
                    println!("{} {}", timestamp, format!("✗ {}", parts.join(", ")).red());
                }

                println!("  {} {:.2}s", "⏱".dimmed(), result.duration.as_secs_f64());
            }
        }
    }
}

/// Count total errors in diagnostics
fn count_errors(diagnostics: &HashMap<PathBuf, Vec<Diagnostic>>) -> usize {
    diagnostics
        .values()
        .flat_map(|diags| diags.iter())
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .count()
}

/// Count errors and warnings in lint diagnostics
fn count_lint_issues(diagnostics: &HashMap<PathBuf, Vec<Diagnostic>>) -> (usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    for diag in diagnostics.values().flat_map(|d| d.iter()) {
        match diag.severity {
            DiagnosticSeverity::Error => errors += 1,
            DiagnosticSeverity::Warning => warnings += 1,
            _ => {}
        }
    }
    (errors, warnings)
}

/// Get current time as string
fn chrono_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now();
    let datetime = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = datetime.as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
