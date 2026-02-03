use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use std::path::PathBuf;

/// A single usage of a deprecated field
#[derive(Clone)]
struct DeprecatedUsage {
    file_path: String,
    line: usize,
    column: usize,
}

/// Grouped deprecation info for a single deprecated element
struct DeprecatedElement {
    name: String,
    reason: Option<String>,
    usages: Vec<DeprecatedUsage>,
}
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "deprecations")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Load project
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

    // Get lint diagnostics and filter for no_deprecated rule
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner(
            "Scanning for deprecated usages...",
        ))
    } else {
        None
    };

    let scan_start = std::time::Instant::now();
    let all_diagnostics = host.all_lint_diagnostics();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    let scan_duration = scan_start.elapsed();

    // Group deprecated usages by field/element name
    let mut deprecated_elements: HashMap<String, DeprecatedElement> = HashMap::new();

    for (file_path, diagnostics) in all_diagnostics {
        for diag in diagnostics {
            // Only process no_deprecated rule diagnostics
            if diag.code.as_deref() != Some("no_deprecated") {
                continue;
            }

            // Parse the diagnostic message to extract field name and reason
            // Message format: "Field 'name' is deprecated" or "Field 'name' is deprecated: reason"
            let (name, reason) = parse_deprecation_message(&diag.message);

            let usage = DeprecatedUsage {
                file_path: file_path.to_string_lossy().to_string(),
                // Convert from 0-based to 1-based for display
                line: (diag.range.start.line + 1) as usize,
                column: (diag.range.start.character + 1) as usize,
            };

            deprecated_elements
                .entry(name.clone())
                .and_modify(|e| e.usages.push(usage.clone()))
                .or_insert_with(|| DeprecatedElement {
                    name,
                    reason,
                    usages: vec![usage],
                });
        }
    }

    // Sort elements by name
    let mut elements: Vec<_> = deprecated_elements.into_values().collect();
    elements.sort_by(|a, b| a.name.cmp(&b.name));

    // Calculate total usages
    let total_usages: usize = elements.iter().map(|e| e.usages.len()).sum();

    // Display results
    let total_duration = start_time.elapsed();

    match format {
        OutputFormat::Human => {
            if elements.is_empty() {
                println!();
                println!("{}", "✓ No deprecated field usages found!".green().bold());
            } else {
                println!();
                println!("{}", "Deprecated Field Usage".bold().underline());
                println!("{}", "======================".dimmed());

                for element in &elements {
                    println!();
                    if let Some(ref reason) = element.reason {
                        println!(
                            "{} {}",
                            element.name.yellow().bold(),
                            format!("(deprecated: \"{reason}\")").dimmed()
                        );
                    } else {
                        println!(
                            "{} {}",
                            element.name.yellow().bold(),
                            "(deprecated)".dimmed()
                        );
                    }

                    for usage in &element.usages {
                        println!("  {} {}:{}", "-".dimmed(), usage.file_path, usage.line);
                    }
                }

                println!();
                println!(
                    "{}",
                    format!(
                        "Found {} deprecated element(s) with {} usage(s)",
                        elements.len(),
                        total_usages
                    )
                    .yellow()
                    .bold()
                );
            }

            println!(
                "  {} load: {:.2}s, scan: {:.2}s, total: {:.2}s",
                "⏱".dimmed(),
                load_duration.as_secs_f64(),
                scan_duration.as_secs_f64(),
                total_duration.as_secs_f64()
            );
        }
        OutputFormat::Json => {
            let json_output: Vec<_> = elements
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name,
                        "reason": e.reason,
                        "usages": e.usages.iter().map(|u| {
                            serde_json::json!({
                                "file": u.file_path,
                                "line": u.line,
                                "column": u.column
                            })
                        }).collect::<Vec<_>>()
                    })
                })
                .collect();

            println!("{}", serde_json::to_string_pretty(&json_output)?);
        }
    }

    Ok(())
}

/// Parse deprecation message to extract element name and reason
fn parse_deprecation_message(message: &str) -> (String, Option<String>) {
    // Message format examples:
    // "Field 'legacyId' is deprecated"
    // "Field 'legacyId' is deprecated: Use id instead"
    // "Argument 'oldArg' is deprecated"
    // "Enum value 'OLD_VALUE' is deprecated: Use NEW_VALUE"

    // Extract the name between single quotes
    let name = message
        .split('\'')
        .nth(1)
        .map_or_else(|| "unknown".to_string(), String::from);

    // Extract the reason after ": "
    let reason = message.split(": ").nth(1).map(String::from);

    (name, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deprecation_message_with_reason() {
        let (name, reason) =
            parse_deprecation_message("Field 'legacyId' is deprecated: Use id instead");
        assert_eq!(name, "legacyId");
        assert_eq!(reason, Some("Use id instead".to_string()));
    }

    #[test]
    fn test_parse_deprecation_message_without_reason() {
        let (name, reason) = parse_deprecation_message("Field 'oldField' is deprecated");
        assert_eq!(name, "oldField");
        assert_eq!(reason, None);
    }

    #[test]
    fn test_parse_deprecation_message_argument() {
        let (name, reason) =
            parse_deprecation_message("Argument 'oldArg' is deprecated: Use newArg");
        assert_eq!(name, "oldArg");
        assert_eq!(reason, Some("Use newArg".to_string()));
    }

    #[test]
    fn test_parse_deprecation_message_enum_value() {
        let (name, reason) =
            parse_deprecation_message("Enum value 'OLD_VALUE' is deprecated: Use NEW_VALUE");
        assert_eq!(name, "OLD_VALUE");
        assert_eq!(reason, Some("Use NEW_VALUE".to_string()));
    }

    #[test]
    fn test_parse_deprecation_message_no_quotes() {
        let (name, reason) = parse_deprecation_message("Some other message format");
        assert_eq!(name, "unknown");
        assert_eq!(reason, None);
    }
}
