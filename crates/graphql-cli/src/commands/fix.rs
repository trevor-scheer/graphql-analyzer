use crate::analysis::CliAnalysisHost;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use graphql_linter::LintDiagnostic;
use std::path::PathBuf;

/// Represents a fix to apply to a file
pub struct FileFix {
    /// The file path
    pub path: PathBuf,
    /// All diagnostics with fixes for this file
    pub diagnostics: Vec<LintDiagnostic>,
}

/// Collect all diagnostics with fixes from the analysis host
pub fn collect_fixable_diagnostics(
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
pub fn display_dry_run(fixes: &[FileFix], format: OutputFormat) {
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
pub fn apply_fixes(fixes: &[FileFix], format: OutputFormat) -> Result<()> {
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
