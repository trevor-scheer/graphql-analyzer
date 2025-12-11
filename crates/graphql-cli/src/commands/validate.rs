use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_project::StaticGraphQLProject;
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

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name.as_ref(), "validate")?;

    // Load projects from config
    let load_projects_span = tracing::info_span!("load_projects");
    let projects_result =
        async { StaticGraphQLProject::from_config_with_base(&ctx.config, &ctx.base_dir).await }
            .instrument(load_projects_span)
            .await;

    let projects = match projects_result {
        Ok(projects) => projects,
        Err(e) => {
            if matches!(format, OutputFormat::Human) {
                eprintln!("{} {}", "✗ Failed to load projects:".red(), e);
            } else {
                eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
            }
            process::exit(1);
        }
    };

    // Select the project to validate (either specified or "default")
    let project_name_to_use = project_name.as_deref().unwrap_or("default");
    let (project_name, project) = projects
        .into_iter()
        .find(|(n, _)| n == project_name_to_use)
        .unwrap_or_else(|| {
            eprintln!(
                "{}",
                format!("Project '{project_name_to_use}' not found").red()
            );
            process::exit(1);
        });

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) {
        let doc_index = project.document_index();
        let op_count = doc_index.operations.len();
        let frag_count = doc_index.fragments.len();
        println!("{}", "✓ Schema loaded successfully".green());
        println!(
            "{} ({} operations, {} fragments)",
            "✓ Documents loaded successfully".green(),
            op_count,
            frag_count
        );
    }

    // Validate all files
    let validate_span = tracing::info_span!("validate_all", project = %project_name);
    let all_diagnostics = async { project.validate_all() }
        .instrument(validate_span)
        .await;

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
    if matches!(format, OutputFormat::Human) {
        println!();
        if total_errors == 0 {
            println!("{}", "✓ All validations passed!".green().bold());
        } else {
            println!("{}", format!("✗ Found {total_errors} error(s)").red());
        }
    }

    if total_errors > 0 {
        process::exit(1);
    }

    Ok(())
}
