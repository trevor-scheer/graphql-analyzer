use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::watch::{FileWatcher, WatchConfig, WatchMode};
use crate::{ExitCode, OutputFormat, OutputOptions};
use anyhow::Result;
use colored::Colorize;
use graphql_ide::DiagnosticSeverity;
use std::path::PathBuf;

#[allow(clippy::too_many_lines)]
#[tracing::instrument(skip(config_path, project_name, format, output_opts), fields(project = ?project_name))]
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    watch: bool,
    syntax_only: bool,
    output_opts: OutputOptions,
) -> Result<()> {
    // Define diagnostic output structure for collecting errors
    struct DiagnosticOutput {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
    }

    if watch {
        return run_watch_mode(config_path, project_name, format);
    }

    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "validate")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Load and select project
    let spinner = if matches!(format, OutputFormat::Human) && output_opts.show_progress {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let load_start = std::time::Instant::now();
    let _load_projects_span = tracing::info_span!("load_projects").entered();
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

    // Strict mode: fail if no schema loaded (unless --syntax-only is set)
    if !host.schema_loaded() && !syntax_only {
        if matches!(format, OutputFormat::Human) {
            eprintln!(
                "{} {}",
                "✗".red().bold(),
                "No schema files found. Cannot validate documents against schema.".red()
            );
            eprintln!(
                "  {}",
                "Use --syntax-only to skip schema validation and only check document syntax."
                    .dimmed()
            );
        } else if matches!(format, OutputFormat::Json) {
            eprintln!(
                "{}",
                serde_json::json!({
                    "error": "No schema files found",
                    "hint": "Use --syntax-only to skip schema validation"
                })
            );
        } else {
            // GitHub Actions format
            eprintln!(
                "::error ::No schema files found. Use --syntax-only to skip schema validation."
            );
        }
        ExitCode::ConfigError.exit();
    }

    // Check if any documents were loaded
    if host.document_count() == 0 {
        if matches!(format, OutputFormat::Human) {
            eprintln!(
                "{} {}",
                "✗".red().bold(),
                "No document files found matching configured patterns.".red()
            );
        } else if matches!(format, OutputFormat::Json) {
            eprintln!(
                "{}",
                serde_json::json!({
                    "error": "No document files found matching configured patterns"
                })
            );
        } else {
            eprintln!("::error ::No document files found matching configured patterns.");
        }
        ExitCode::ConfigError.exit();
    }

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) && output_opts.show_info {
        if host.schema_loaded() {
            println!("{}", "✓ Schema loaded successfully".green());
        } else if syntax_only {
            println!(
                "{}",
                "! Syntax-only mode: schema validation skipped".yellow()
            );
        }
        println!("{}", "✓ Documents loaded successfully".green());
    }

    // Validate all files (spec validation only, no custom lints)
    let spinner = if matches!(format, OutputFormat::Human) && output_opts.show_progress {
        Some(crate::progress::spinner("Validating GraphQL documents..."))
    } else {
        None
    };

    let validate_start = std::time::Instant::now();
    let _validate_span = tracing::info_span!("validate_all").entered();
    let all_diagnostics = host.all_validation_diagnostics();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let validate_duration = validate_start.elapsed();

    tracing::info!(
        files_with_diagnostics = all_diagnostics.len(),
        "Validation completed"
    );

    // Convert diagnostics to CLI output format, grouped by file
    let mut files_with_errors: std::collections::HashMap<String, Vec<DiagnosticOutput>> =
        std::collections::HashMap::new();

    for (file_path, diagnostics) in all_diagnostics {
        let file_path_str = file_path.to_string_lossy().to_string();
        for diag in diagnostics {
            // Only process errors
            if diag.severity == DiagnosticSeverity::Error {
                let diag_output = DiagnosticOutput {
                    file_path: file_path_str.clone(),
                    // graphql-ide uses 0-based, CLI output uses 1-based
                    line: (diag.range.start.line + 1) as usize,
                    column: (diag.range.start.character + 1) as usize,
                    message: diag.message,
                };

                files_with_errors
                    .entry(file_path_str.clone())
                    .or_default()
                    .push(diag_output);
            }
        }
    }

    // Flatten for counting and human output
    let all_errors: Vec<_> = files_with_errors.values().flatten().collect();
    let total_errors = all_errors.len();
    let total_files = files_with_errors.len();

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
            // Build aggregated JSON output
            let mut files: Vec<serde_json::Value> = files_with_errors
                .iter()
                .map(|(file, errors)| {
                    let errors_json: Vec<serde_json::Value> = errors
                        .iter()
                        .map(|e| {
                            let mut error_obj = serde_json::json!({
                                "message": e.message,
                                "severity": "error"
                            });
                            if e.line > 0 {
                                error_obj["location"] = serde_json::json!({
                                    "line": e.line,
                                    "column": e.column
                                });
                            }
                            error_obj
                        })
                        .collect();

                    serde_json::json!({
                        "file": file,
                        "errors": errors_json
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
                    "total_errors": total_errors
                }
            });

            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Github => {
            // Print GitHub Actions workflow commands
            for error in &all_errors {
                if error.line > 0 {
                    println!(
                        "::error file={},line={},col={}::{}",
                        error.file_path, error.line, error.column, error.message
                    );
                } else {
                    println!("::error ::{}", error.message);
                }
            }
        }
    }

    // Summary
    let total_duration = start_time.elapsed();
    if matches!(format, OutputFormat::Human) && output_opts.show_info {
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
        ExitCode::ValidationError.exit();
    }

    Ok(())
}

/// Run validate in watch mode
fn run_watch_mode(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Load config
    let ctx = CommandContext::load(config_path, project_name, "validate")?;

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
        mode: WatchMode::Validate,
        format,
        project_config,
        base_dir: ctx.base_dir,
    };

    // Create and run watcher
    let mut watcher = FileWatcher::new(watch_config)?;
    watcher.start()?;
    watcher.run()
}
