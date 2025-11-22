use crate::OutputFormat;
use anyhow::{Context, Result};
use colored::Colorize;
use graphql_config::{find_config, load_config};
use graphql_project::GraphQLProject;
use std::path::PathBuf;
use std::process;

#[allow(clippy::too_many_lines)] // Main validation logic - will refactor when more features are added
pub async fn run(
    config_path: Option<PathBuf>,
    project_name: Option<String>,
    format: OutputFormat,
    watch: bool,
) -> Result<()> {
    if watch {
        println!("{}", "Watch mode not yet implemented".yellow());
        return Ok(());
    }

    // Find and load config
    let config_path = if let Some(path) = config_path {
        path
    } else {
        let current_dir = std::env::current_dir()?;
        find_config(&current_dir)
            .context("Failed to search for config")?
            .context("No GraphQL config file found")?
    };

    let config = load_config(&config_path).context("Failed to load config")?;

    // Get projects
    let projects = GraphQLProject::from_config(&config)?;

    // Filter by project name if specified
    let projects_to_validate: Vec<_> = if let Some(ref name) = project_name {
        projects.into_iter().filter(|(n, _)| n == name).collect()
    } else {
        projects
    };

    if projects_to_validate.is_empty() {
        if let Some(name) = project_name {
            eprintln!("{}", format!("Project '{name}' not found").red());
            process::exit(1);
        }
    }

    let mut total_errors = 0;

    for (name, project) in &projects_to_validate {
        if projects_to_validate.len() > 1 {
            println!("\n{}", format!("=== Project: {name} ===").bold().cyan());
        }

        // Load schema
        match project.load_schema().await {
            Ok(()) => {
                if matches!(format, OutputFormat::Human) {
                    println!("{}", "✓ Schema loaded successfully".green());
                }
            }
            Err(e) => {
                if matches!(format, OutputFormat::Human) {
                    eprintln!("{} {}", "✗ Schema error:".red(), e);
                } else {
                    eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
                }
                process::exit(1);
            }
        }

        // Load documents
        match project.load_documents() {
            Ok(()) => {
                if matches!(format, OutputFormat::Human) {
                    println!("{}", "✓ Documents loaded successfully".green());
                }
            }
            Err(e) => {
                if matches!(format, OutputFormat::Human) {
                    eprintln!("{} {}", "✗ Document error:".red(), e);
                } else {
                    eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
                }
                process::exit(1);
            }
        }

        // Validate all loaded documents
        let document_index = project.get_document_index();

        // Collect all unique file paths from operations and fragments
        let mut file_paths = std::collections::HashSet::new();
        for op_info in document_index.operations.values() {
            file_paths.insert(&op_info.file_path);
        }
        for frag_info in document_index.fragments.values() {
            file_paths.insert(&frag_info.file_path);
        }

        // Validate each file
        for file_path in file_paths {
            // Read the file contents
            let contents = match std::fs::read_to_string(file_path) {
                Ok(contents) => contents,
                Err(e) => {
                    eprintln!("{} {}: {}", "✗ Failed to read".red(), file_path, e);
                    continue;
                }
            };

            // For now, just validate the raw file contents as GraphQL
            // TODO: Use graphql-extract once it's available in the CLI
            let extracted = vec![contents];

            // Validate each extracted GraphQL document
            for (doc_index, source) in extracted.iter().enumerate() {
                match project.validate_document(source) {
                    Ok(()) => {
                        // Valid document - no output in human mode unless verbose
                    }
                    Err(diagnostics) => {
                        // Found validation errors
                        for diagnostic in diagnostics.iter() {
                            total_errors += 1;

                            match format {
                                OutputFormat::Human => {
                                    // Use DiagnosticList's built-in Display formatting
                                    println!(
                                        "{} {}{}",
                                        "error:".red().bold(),
                                        file_path,
                                        if extracted.len() > 1 {
                                            format!(" (document {})", doc_index + 1)
                                        } else {
                                            String::new()
                                        }
                                    );
                                    println!("{}", diagnostic);
                                }
                                OutputFormat::Json => {
                                    // For JSON output, format as structured data
                                    println!(
                                        "{}",
                                        serde_json::json!({
                                            "file": file_path,
                                            "document_index": doc_index,
                                            "error": format!("{}", diagnostic.error),
                                            "location": diagnostic.line_column_range().map(|range| {
                                                serde_json::json!({
                                                    "start": {
                                                        "line": range.start.line,
                                                        "column": range.start.column
                                                    },
                                                    "end": {
                                                        "line": range.end.line,
                                                        "column": range.end.column
                                                    }
                                                })
                                            })
                                        })
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Summary
    if matches!(format, OutputFormat::Human) {
        println!();
        if total_errors == 0 {
            println!("{}", "✓ All validations passed!".green().bold());
        } else {
            println!(
                "{}",
                format!("Found {total_errors} error(s)").yellow()
            );
        }
    }

    if total_errors > 0 {
        process::exit(1);
    }

    Ok(())
}
