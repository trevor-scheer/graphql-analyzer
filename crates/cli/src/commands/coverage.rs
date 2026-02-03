use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::process;

#[allow(clippy::needless_pass_by_value)]
#[tracing::instrument(skip(config_path, project_name, format), fields(project = ?project_name))]
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
    filter_type: Option<String>,
) -> Result<()> {
    let filter_type = filter_type.as_deref();
    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "coverage")?;

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

    let host = CliAnalysisHost::from_project_config(&project_config, &ctx.base_dir)
        .map_err(|e| {
            if matches!(format, OutputFormat::Human) {
                eprintln!("{} {}", "Error loading project:".red(), e);
            } else {
                eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
            }
            process::exit(1);
        })
        .unwrap();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Analyze field coverage
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Analyzing field coverage..."))
    } else {
        None
    };

    let Some(coverage) = host.field_coverage() else {
        if let Some(pb) = spinner {
            pb.finish_and_clear();
        }
        eprintln!("{}", "No project files loaded".red());
        process::exit(1);
    };

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let total_duration = start_time.elapsed();

    match format {
        OutputFormat::Human => {
            print_human_report(&coverage, filter_type, total_duration);
        }
        OutputFormat::Json | OutputFormat::Github => {
            print_json_report(&coverage, filter_type);
        }
    }

    Ok(())
}

fn print_human_report(
    coverage: &graphql_ide::FieldCoverageReport,
    filter_type: Option<&str>,
    duration: std::time::Duration,
) {
    println!();
    println!("{}", "Schema Coverage Report".bold());
    println!("{}", "======================".dimmed());
    println!();

    // Overall stats
    let percentage = coverage.coverage_percentage();
    let percentage_str = format!("{percentage:.1}%");
    let color_percentage = if percentage >= 80.0 {
        percentage_str.green()
    } else if percentage >= 50.0 {
        percentage_str.yellow()
    } else {
        percentage_str.red()
    };

    println!(
        "Overall: {} ({}/{} fields)",
        color_percentage.bold(),
        coverage.used_fields,
        coverage.total_fields
    );
    println!();

    // Type coverage
    println!("{}", "Type Coverage:".bold());

    // Sort types by name for consistent output
    let mut types: Vec<_> = coverage.types.iter().collect();
    types.sort_by(|a, b| a.type_name.cmp(&b.type_name));

    for type_info in types {
        // Filter by type if specified
        if let Some(filter) = filter_type {
            if type_info.type_name != filter {
                continue;
            }
        }

        let type_percentage = type_info.coverage_percentage();
        let type_percentage_str = format!("{type_percentage:.0}%");
        let color_type_percentage = if type_percentage >= 80.0 {
            type_percentage_str.green()
        } else if type_percentage >= 50.0 {
            type_percentage_str.yellow()
        } else {
            type_percentage_str.red()
        };

        println!(
            "  {:<20} {} ({}/{} fields)",
            format!("{}:", type_info.type_name),
            color_type_percentage,
            type_info.used_fields,
            type_info.total_fields
        );
    }

    // Unused fields
    let unused = coverage.unused_fields();
    if !unused.is_empty() {
        println!();
        println!("{}", "Unused Fields:".bold());

        // Sort by type name, then field name
        let mut unused_sorted = unused.clone();
        unused_sorted.sort();

        for (type_name, field_name) in &unused_sorted {
            // Filter by type if specified
            if let Some(filter) = filter_type {
                if type_name != filter {
                    continue;
                }
            }
            println!("  {} {}.{}", "-".dimmed(), type_name.yellow(), field_name);
        }
    }

    println!();
    println!(
        "{} Analysis completed in {:.2}s",
        "".dimmed(),
        duration.as_secs_f64()
    );
}

fn print_json_report(coverage: &graphql_ide::FieldCoverageReport, filter_type: Option<&str>) {
    // Build JSON structure
    let mut type_coverage: Vec<serde_json::Value> = coverage
        .types
        .iter()
        .filter(|t| filter_type.is_none_or(|f| t.type_name == f))
        .map(|t| {
            serde_json::json!({
                "type": t.type_name,
                "totalFields": t.total_fields,
                "usedFields": t.used_fields,
                "coverage": t.coverage_percentage()
            })
        })
        .collect();

    // Sort for consistent output
    type_coverage.sort_by(|a, b| {
        a.get("type")
            .and_then(|v| v.as_str())
            .cmp(&b.get("type").and_then(|v| v.as_str()))
    });

    let mut unused_fields: Vec<serde_json::Value> = coverage
        .unused_fields()
        .iter()
        .filter(|(type_name, _)| filter_type.is_none_or(|f| type_name == f))
        .map(|(type_name, field_name)| {
            serde_json::json!({
                "type": type_name,
                "field": field_name
            })
        })
        .collect();

    // Sort for consistent output
    unused_fields.sort_by(|a, b| {
        let a_key = (
            a.get("type").and_then(|v| v.as_str()),
            a.get("field").and_then(|v| v.as_str()),
        );
        let b_key = (
            b.get("type").and_then(|v| v.as_str()),
            b.get("field").and_then(|v| v.as_str()),
        );
        a_key.cmp(&b_key)
    });

    // Build field usages for detailed output
    let mut field_usages: Vec<serde_json::Value> = coverage
        .field_usages
        .iter()
        .filter(|((type_name, _), _)| filter_type.is_none_or(|f| type_name == f))
        .filter(|(_, info)| info.usage_count > 0)
        .map(|((type_name, field_name), info)| {
            serde_json::json!({
                "type": type_name,
                "field": field_name,
                "usageCount": info.usage_count,
                "operations": info.operations
            })
        })
        .collect();

    // Sort for consistent output
    field_usages.sort_by(|a, b| {
        let a_key = (
            a.get("type").and_then(|v| v.as_str()),
            a.get("field").and_then(|v| v.as_str()),
        );
        let b_key = (
            b.get("type").and_then(|v| v.as_str()),
            b.get("field").and_then(|v| v.as_str()),
        );
        a_key.cmp(&b_key)
    });

    let report = serde_json::json!({
        "summary": {
            "totalFields": coverage.total_fields,
            "usedFields": coverage.used_fields,
            "coverage": coverage.coverage_percentage()
        },
        "typeCoverage": type_coverage,
        "unusedFields": unused_fields,
        "fieldUsages": field_usages
    });

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
