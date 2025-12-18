use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_linter::{DocumentSchemaContext, LintConfig, Linter};
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

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name.as_ref(), "lint")?;

    // Load and select project
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let (_project_name, project) = ctx.load_project(project_name.as_deref()).await?;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

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

    // Get extract config
    let extract_config = project.get_extract_config();

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

    // Run lints on each file
    for file_path in all_file_paths {
        if let Some(ref pb) = progress {
            pb.inc(1);
        }
        // Use graphql-extract to extract GraphQL from the file
        let extracted = match graphql_extract::extract_from_file(
            std::path::Path::new(file_path),
            &extract_config,
        ) {
            Ok(items) => items,
            Err(e) => {
                if matches!(format, OutputFormat::Human) {
                    eprintln!(
                        "{} {}: {}",
                        "✗ Failed to extract GraphQL from".red(),
                        file_path,
                        e
                    );
                }
                continue;
            }
        };

        if extracted.is_empty() {
            continue;
        }

        let schema_index = project.get_schema_index();

        // Run lints on each extracted block
        for block in &extracted {
            // Run standalone document rules (don't need schema, but need fragments)
            let standalone_ctx = graphql_linter::StandaloneDocumentContext {
                document: &block.source,
                file_name: file_path,
                fragments: Some(document_index),
            };
            let standalone_diagnostics = linter.lint_standalone_document(&standalone_ctx);

            // Convert standalone diagnostics to output format
            for diag in standalone_diagnostics {
                // Adjust positions for extracted blocks
                let adjusted_line = block.location.range.start.line + diag.range.start.line;
                let adjusted_col = if diag.range.start.line == 0 {
                    block.location.range.start.column + diag.range.start.character
                } else {
                    diag.range.start.character
                };

                let adjusted_end_line = block.location.range.start.line + diag.range.end.line;
                let adjusted_end_col = if diag.range.end.line == 0 {
                    block.location.range.start.column + diag.range.end.character
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

            // Run document+schema rules
            let ctx = DocumentSchemaContext {
                document: &block.source,
                file_name: file_path,
                schema: schema_index,
            };
            let diagnostics = linter.lint_document(&ctx);

            // Convert diagnostics to output format
            for diag in diagnostics {
                // Adjust positions for extracted blocks
                let adjusted_line = block.location.range.start.line + diag.range.start.line;
                let adjusted_col = if diag.range.start.line == 0 {
                    block.location.range.start.column + diag.range.start.character
                } else {
                    diag.range.start.character
                };

                let adjusted_end_line = block.location.range.start.line + diag.range.end.line;
                let adjusted_end_col = if diag.range.end.line == 0 {
                    block.location.range.start.column + diag.range.end.character
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
    let ctx = graphql_linter::ProjectContext {
        documents: document_index,
        schema: schema_index,
    };
    let project_diagnostics = linter.lint_project(&ctx);

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

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
    }

    if total_errors > 0 {
        process::exit(1);
    }

    Ok(())
}
