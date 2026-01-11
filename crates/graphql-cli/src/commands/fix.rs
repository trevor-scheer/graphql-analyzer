use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_linter::LintDiagnostic;
use std::path::PathBuf;

/// Represents a fix to apply to a file
struct FileFix {
    /// The file path
    path: PathBuf,
    /// All diagnostics with fixes for this file
    diagnostics: Vec<LintDiagnostic>,
}

#[allow(clippy::needless_pass_by_value)] // rule_filter comes from clap and can't be borrowed
pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    dry_run: bool,
    rule_filter: Option<Vec<String>>,
    format: OutputFormat,
) -> Result<()> {
    // Start timing
    let start_time = std::time::Instant::now();

    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "fix")?;

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

    let host = CliAnalysisHost::from_project_config(&project_config, &ctx.base_dir)?;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Report project loaded successfully
    if matches!(format, OutputFormat::Human) {
        println!("{}", "✓ Schema loaded successfully".green());
        println!("{}", "✓ Documents loaded successfully".green());
    }

    // Collect diagnostics with fixes
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Analyzing lint issues..."))
    } else {
        None
    };

    let fixes = collect_fixable_diagnostics(&host, rule_filter.as_deref());

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Count fixable issues
    let total_fixes: usize = fixes.iter().map(|f| f.diagnostics.len()).sum();

    if total_fixes == 0 {
        if matches!(format, OutputFormat::Human) {
            println!("{}", "✓ No fixable lint issues found!".green().bold());
        }
        return Ok(());
    }

    // Apply or preview fixes
    if dry_run {
        display_dry_run(&fixes, format);
    } else {
        apply_fixes(&fixes, format)?;
    }

    // Summary
    let total_duration = start_time.elapsed();
    if matches!(format, OutputFormat::Human) {
        println!();
        let action = if dry_run { "would fix" } else { "fixed" };
        println!(
            "{}",
            format!(
                "✓ {} {} issue(s) in {} file(s)",
                action,
                total_fixes,
                fixes.len()
            )
            .green()
            .bold()
        );
        println!(
            "  {} total: {:.2}s",
            "⏱".dimmed(),
            total_duration.as_secs_f64()
        );
    }

    Ok(())
}

/// Collect all diagnostics with fixes from the analysis host
fn collect_fixable_diagnostics(
    host: &CliAnalysisHost,
    rule_filter: Option<&[String]>,
) -> Vec<FileFix> {
    let all_diagnostics = host.all_lint_diagnostics_with_fixes();

    let mut fixes = Vec::new();

    for (path, diagnostics) in all_diagnostics {
        // Filter to only fixable diagnostics
        let fixable: Vec<_> = diagnostics
            .into_iter()
            .filter(|d| {
                // Must have a fix
                if !d.has_fix() {
                    return false;
                }

                // Must match rule filter if provided
                if let Some(rules) = rule_filter {
                    return rules.iter().any(|r| r == &d.rule);
                }

                true
            })
            .collect();

        if !fixable.is_empty() {
            fixes.push(FileFix {
                path,
                diagnostics: fixable,
            });
        }
    }

    fixes
}

/// Display what would be fixed in dry-run mode
fn display_dry_run(fixes: &[FileFix], format: OutputFormat) {
    match format {
        OutputFormat::Human => {
            println!();
            println!("{}", "Dry run - would fix the following issues:".cyan());
            println!();

            for file_fix in fixes {
                println!("{}:", file_fix.path.display().to_string().bold());
                for diag in &file_fix.diagnostics {
                    let fix = diag.fix.as_ref().unwrap();
                    println!("  {} {} ({})", "→".green(), fix.label, diag.rule.dimmed());
                }
                println!();
            }
        }
        OutputFormat::Json => {
            for file_fix in fixes {
                for diag in &file_fix.diagnostics {
                    let fix = diag.fix.as_ref().unwrap();
                    println!(
                        "{}",
                        serde_json::json!({
                            "action": "would_fix",
                            "file": file_fix.path.to_string_lossy(),
                            "rule": diag.rule,
                            "fix": fix.label,
                            "offset_start": diag.offset_range.start,
                            "offset_end": diag.offset_range.end,
                        })
                    );
                }
            }
        }
    }
}

/// Apply fixes to files
fn apply_fixes(fixes: &[FileFix], format: OutputFormat) -> Result<()> {
    for file_fix in fixes {
        apply_file_fixes(file_fix, format)?;
    }

    Ok(())
}

/// Apply all fixes to a single file
fn apply_file_fixes(file_fix: &FileFix, format: OutputFormat) -> Result<()> {
    // Read the file content
    let content = std::fs::read_to_string(&file_fix.path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", file_fix.path.display(), e))?;

    // Collect all edits from all diagnostics
    let mut all_edits: Vec<_> = file_fix
        .diagnostics
        .iter()
        .filter_map(|d| d.fix.as_ref())
        .flat_map(|f| f.edits.iter())
        .collect();

    // Sort edits by start position in reverse order so we can apply them from end to start
    // This ensures earlier edits don't shift the positions of later edits
    all_edits.sort_by(|a, b| b.offset_range.start.cmp(&a.offset_range.start));

    // Apply edits
    let mut result = content.clone();
    for edit in &all_edits {
        // Validate range is within bounds
        if edit.offset_range.end > result.len() {
            tracing::warn!(
                file = %file_fix.path.display(),
                start = edit.offset_range.start,
                end = edit.offset_range.end,
                len = result.len(),
                "Edit range out of bounds, skipping"
            );
            continue;
        }

        // Apply the edit
        result = format!(
            "{}{}{}",
            &result[..edit.offset_range.start],
            edit.new_text,
            &result[edit.offset_range.end..]
        );
    }

    // Write the fixed content back to the file
    std::fs::write(&file_fix.path, &result)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", file_fix.path.display(), e))?;

    // Report what was fixed
    match format {
        OutputFormat::Human => {
            println!(
                "{} {} ({})",
                "✓".green(),
                file_fix.path.display(),
                format!("{} fix(es)", file_fix.diagnostics.len()).dimmed()
            );
        }
        OutputFormat::Json => {
            for diag in &file_fix.diagnostics {
                let fix = diag.fix.as_ref().unwrap();
                println!(
                    "{}",
                    serde_json::json!({
                        "action": "fixed",
                        "file": file_fix.path.to_string_lossy(),
                        "rule": diag.rule,
                        "fix": fix.label,
                    })
                );
            }
        }
    }

    Ok(())
}
