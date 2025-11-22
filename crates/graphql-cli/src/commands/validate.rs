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

    // Get the base directory from the config path
    let base_dir = config_path
        .parent()
        .context("Failed to get config directory")?
        .to_path_buf();

    // Get projects with base directory
    let projects = GraphQLProject::from_config_with_base(&config, base_dir)?;

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
                    let schema = project.get_schema();
                    if let Some(ref schema_str) = schema {
                        let line_count = schema_str.lines().count();
                        println!("{} ({} lines)", "✓ Schema loaded successfully".green(), line_count);
                    } else {
                        println!("{}", "✓ Schema loaded successfully".green());
                    }
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
                    let doc_index = project.get_document_index();
                    let op_count = doc_index.operations.len();
                    let frag_count = doc_index.fragments.len();
                    println!(
                        "{} ({} operations, {} fragments)",
                        "✓ Documents loaded successfully".green(),
                        op_count,
                        frag_count
                    );
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
            // Use graphql-extract to extract GraphQL from the file
            // This handles both .graphql files and embedded GraphQL in TypeScript/JavaScript
            let extracted = match graphql_extract::extract_from_file(
                std::path::Path::new(file_path),
                &graphql_extract::ExtractConfig::default(),
            ) {
                Ok(items) => items,
                Err(e) => {
                    eprintln!(
                        "{} {}: {}",
                        "✗ Failed to extract GraphQL from".red(),
                        file_path,
                        e
                    );
                    continue;
                }
            };

            if extracted.is_empty() {
                if matches!(format, OutputFormat::Human) {
                    eprintln!(
                        "{} {} (no GraphQL found)",
                        "⚠".yellow(),
                        file_path
                    );
                }
                continue;
            }

            // Validate each extracted GraphQL document
            for item in extracted.iter() {
                let source = &item.source;

                // Skip documents that only contain fragments
                // Fragments need to be validated in the context of an operation
                if source.trim_start().starts_with("fragment") && !source.contains("query") && !source.contains("mutation") && !source.contains("subscription") {
                    continue;
                }

                // Validate with the actual file path and line offset
                // This makes apollo-compiler's diagnostics show the correct file:line:column
                let line_offset = item.location.range.start.line;
                let validation_result = project.validate_document_with_location(
                    source,
                    file_path,
                    line_offset,
                );

                match validation_result {
                    Ok(()) => {
                        // Valid document - no output in human mode unless verbose
                    }
                    Err(diagnostics) => {
                        // Found validation errors - diagnostics already have correct file and line numbers
                        for diagnostic in diagnostics.iter() {
                            total_errors += 1;

                            match format {
                                OutputFormat::Human => {
                                    // Just print the diagnostic - it already has the correct location
                                    println!("\n{}", diagnostic);
                                }
                                OutputFormat::Json => {
                                    // For JSON output, extract location from diagnostic
                                    let location = diagnostic.line_column_range().map(|range| {
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
                                    });

                                    println!(
                                        "{}",
                                        serde_json::json!({
                                            "file": file_path,
                                            "error": format!("{}", diagnostic.error),
                                            "location": location
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
