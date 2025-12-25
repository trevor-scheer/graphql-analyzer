use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_linter::{LintConfig, Linter};
use graphql_project::Severity;
use std::path::PathBuf;
use std::process;

#[allow(clippy::too_many_lines)]
pub async fn run(
    config_path: Option<PathBuf>,
    project_name: Option<String>,
    format: OutputFormat,
    _watch: bool,
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
    let ctx = CommandContext::load(config_path, project_name.as_ref(), "lint")?;

    // Load and select project
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let load_start = std::time::Instant::now();
    let (_project_name, project) = ctx.load_project(project_name.as_deref()).await?;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let load_duration = load_start.elapsed();

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) {
        CommandContext::print_success_message(&project);
    }

    // Get lint config and create linter
    // For multi-project configs, use the selected project's lint config
    // For single-project configs, use the top-level lint config
    let base_lint_config: LintConfig = project
        .lint_config()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();

    let cli_lint_config = project
        .extensions()
        .and_then(|ext| ext.get("cli"))
        .and_then(|cli_ext| {
            if let serde_json::Value::Object(map) = cli_ext {
                map.get("lint")
            } else {
                None
            }
        })
        .and_then(|value| serde_json::from_value::<LintConfig>(value.clone()).ok());

    let lint_config = if let Some(cli_overrides) = cli_lint_config {
        base_lint_config.merge(&cli_overrides)
    } else {
        base_lint_config
    };

    let linter = Linter::new(lint_config);

    // Collect unique file paths that contain operations or fragments
    let document_index = project.get_document_index();
    let mut all_file_paths = std::collections::HashSet::new();
    for op_infos in document_index.operations.values() {
        for op_info in op_infos {
            all_file_paths.insert(&op_info.file_path);
        }
    }
    for frag_infos in document_index.fragments.values() {
        for frag_info in frag_infos {
            all_file_paths.insert(&frag_info.file_path);
        }
    }

    let mut all_warnings = Vec::new();
    let mut all_errors = Vec::new();

    // Create progress bar for file processing
    let progress = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::progress_bar(
            all_file_paths.len() as u64,
            "Linting files",
        ))
    } else {
        None
    };

    let lint_start = std::time::Instant::now();

    // Run lints on each file using cached parsed ASTs
    for file_path in all_file_paths {
        if let Some(ref pb) = progress {
            pb.inc(1);
        }

        let schema_index = project.get_schema_index();

        // Check if we have cached extracted blocks (TypeScript/JavaScript files)
        if let Some(cached_blocks) = document_index.extracted_blocks.get(file_path) {
            // Use cached extracted blocks with their pre-parsed ASTs
            for block in cached_blocks {
                // Run standalone document rules with cached AST
                let standalone_diagnostics = linter.lint_standalone_document(
                    &block.content,
                    file_path,
                    Some(document_index),
                    Some(&block.parsed),
                );

                // Convert standalone diagnostics to output format
                for diag in standalone_diagnostics {
                    // Adjust positions for extracted blocks
                    let adjusted_line = block.start_line + diag.range.start.line;
                    let adjusted_col = if diag.range.start.line == 0 {
                        block.start_column + diag.range.start.character
                    } else {
                        diag.range.start.character
                    };

                    let adjusted_end_line = block.start_line + diag.range.end.line;
                    let adjusted_end_col = if diag.range.end.line == 0 {
                        block.start_column + diag.range.end.character
                    } else {
                        diag.range.end.character
                    };

                    let severity_string = match diag.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                        Severity::Information => "info",
                        Severity::Hint => "hint",
                    }
                    .to_string();

                    let diag_output = DiagnosticOutput {
                        file_path: file_path.clone(),
                        // Convert from 0-based to 1-based for display
                        line: adjusted_line + 1,
                        column: adjusted_col + 1,
                        end_line: adjusted_end_line + 1,
                        end_column: adjusted_end_col + 1,
                        message: diag.message.clone(),
                        severity: severity_string.clone(),
                        rule: diag.code.clone(),
                    };

                    match diag.severity {
                        Severity::Warning | Severity::Information | Severity::Hint => {
                            all_warnings.push(diag_output);
                        }
                        Severity::Error => all_errors.push(diag_output),
                    }
                }

                // Run document+schema rules with cached AST
                let diagnostics = linter.lint_document(
                    &block.content,
                    file_path,
                    schema_index,
                    Some(document_index),
                    Some(&block.parsed),
                );

                // Convert diagnostics to output format
                for diag in diagnostics {
                    // Adjust positions for extracted blocks
                    let adjusted_line = block.start_line + diag.range.start.line;
                    let adjusted_col = if diag.range.start.line == 0 {
                        block.start_column + diag.range.start.character
                    } else {
                        diag.range.start.character
                    };

                    let adjusted_end_line = block.start_line + diag.range.end.line;
                    let adjusted_end_col = if diag.range.end.line == 0 {
                        block.start_column + diag.range.end.character
                    } else {
                        diag.range.end.character
                    };

                    let diag_output = DiagnosticOutput {
                        file_path: file_path.clone(),
                        // Convert from 0-based to 1-based for display
                        line: adjusted_line + 1,
                        column: adjusted_col + 1,
                        end_line: adjusted_end_line + 1,
                        end_column: adjusted_end_col + 1,
                        message: diag.message,
                        severity: match diag.severity {
                            Severity::Error => "error".to_string(),
                            Severity::Warning => "warning".to_string(),
                            Severity::Information => "info".to_string(),
                            Severity::Hint => "hint".to_string(),
                        },
                        rule: diag.code.clone(),
                    };

                    match diag.severity {
                        Severity::Warning | Severity::Information | Severity::Hint => {
                            all_warnings.push(diag_output);
                        }
                        Severity::Error => {
                            all_errors.push(diag_output);
                        }
                    }
                }
            }
        } else if let Some(cached_ast) = document_index.parsed_asts.get(file_path) {
            // Pure .graphql file - use cached parsed AST
            // Read the file content (we still need the source text for linting)
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    if matches!(format, OutputFormat::Human) {
                        eprintln!("{} {}: {}", "✗ Failed to read file".red(), file_path, e);
                    }
                    continue;
                }
            };

            // Run standalone document rules with cached AST
            let standalone_diagnostics = linter.lint_standalone_document(
                &content,
                file_path,
                Some(document_index),
                Some(cached_ast),
            );

            // Convert standalone diagnostics to output format
            for diag in standalone_diagnostics {
                let diag_output = DiagnosticOutput {
                    file_path: file_path.clone(),
                    // Convert from 0-based to 1-based for display
                    line: diag.range.start.line + 1,
                    column: diag.range.start.character + 1,
                    end_line: diag.range.end.line + 1,
                    end_column: diag.range.end.character + 1,
                    message: diag.message.clone(),
                    severity: match diag.severity {
                        Severity::Error => "error".to_string(),
                        Severity::Warning => "warning".to_string(),
                        Severity::Information => "info".to_string(),
                        Severity::Hint => "hint".to_string(),
                    },
                    rule: diag.code.clone(),
                };

                match diag.severity {
                    Severity::Warning | Severity::Information | Severity::Hint => {
                        all_warnings.push(diag_output);
                    }
                    Severity::Error => all_errors.push(diag_output),
                }
            }

            // Run document+schema rules with cached AST
            let diagnostics = linter.lint_document(
                &content,
                file_path,
                schema_index,
                Some(document_index),
                Some(cached_ast),
            );

            // Convert diagnostics to output format
            for diag in diagnostics {
                let diag_output = DiagnosticOutput {
                    file_path: file_path.clone(),
                    // Convert from 0-based to 1-based for display
                    line: diag.range.start.line + 1,
                    column: diag.range.start.character + 1,
                    end_line: diag.range.end.line + 1,
                    end_column: diag.range.end.character + 1,
                    message: diag.message,
                    severity: match diag.severity {
                        Severity::Error => "error".to_string(),
                        Severity::Warning => "warning".to_string(),
                        Severity::Information => "info".to_string(),
                        Severity::Hint => "hint".to_string(),
                    },
                    rule: diag.code.clone(),
                };

                match diag.severity {
                    Severity::Warning | Severity::Information | Severity::Hint => {
                        all_warnings.push(diag_output);
                    }
                    Severity::Error => {
                        all_errors.push(diag_output);
                    }
                }
            }
        } else {
            // Fallback: No cached data found (shouldn't happen in normal operation)
            if matches!(format, OutputFormat::Human) {
                eprintln!(
                    "{} {}",
                    "✗ Warning: No cached data found for".yellow(),
                    file_path
                );
            }
        }
    }

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    // Run project-wide lint rules (e.g., unused_fields, unique_names)
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner(
            "Running project-wide lint rules...",
        ))
    } else {
        None
    };

    let document_index = project.get_document_index();
    let schema_index = project.get_schema_index();
    let lint_ctx = graphql_linter::ProjectContext {
        documents: document_index,
        schema: schema_index,
    };
    let project_diagnostics = linter.lint_project(&lint_ctx);

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let lint_duration = lint_start.elapsed();

    // Flatten the HashMap<String, Vec<Diagnostic>> into Vec<Diagnostic>
    for (file_path, diagnostics) in project_diagnostics {
        for diag in diagnostics {
            let diag_output = DiagnosticOutput {
                file_path: file_path.clone(),
                // Convert from 0-indexed to 1-indexed for display
                line: diag.range.start.line + 1,
                column: diag.range.start.character + 1,
                end_line: diag.range.end.line + 1,
                end_column: diag.range.end.character + 1,
                message: diag.message,
                severity: match diag.severity {
                    Severity::Error => "error".to_string(),
                    Severity::Warning => "warning".to_string(),
                    Severity::Information => "info".to_string(),
                    Severity::Hint => "hint".to_string(),
                },
                rule: diag.code.clone(),
            };

            match diag.severity {
                Severity::Warning | Severity::Information | Severity::Hint => {
                    all_warnings.push(diag_output);
                }
                Severity::Error => {
                    all_errors.push(diag_output);
                }
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
