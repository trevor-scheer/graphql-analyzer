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
        OutputFormat::Json | OutputFormat::Github => {
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

/// A text edit with file-relative positions (adjusted for block offset if applicable)
struct FileRelativeEdit {
    /// File-relative start position
    start: usize,
    /// File-relative end position
    end: usize,
    /// The replacement text
    new_text: String,
}

/// Apply all fixes to a single file
fn apply_file_fixes(file_fix: &FileFix, format: OutputFormat) -> Result<()> {
    // Read the file content
    let content = std::fs::read_to_string(&file_fix.path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", file_fix.path.display(), e))?;

    // Collect all edits from all diagnostics, adjusting for block offsets
    // For embedded GraphQL (TypeScript/JavaScript), edit offsets are relative to the
    // GraphQL block, not the file. We need to add the block_byte_offset to get
    // file-relative positions.
    let mut all_edits: Vec<FileRelativeEdit> = Vec::new();

    for diag in &file_fix.diagnostics {
        let Some(fix) = &diag.fix else { continue };
        let block_offset = diag.block_byte_offset.unwrap_or(0);

        for edit in &fix.edits {
            all_edits.push(FileRelativeEdit {
                start: edit.offset_range.start + block_offset,
                end: edit.offset_range.end + block_offset,
                new_text: edit.new_text.clone(),
            });
        }
    }

    // Sort edits by start position in reverse order so we can apply them from end to start
    // This ensures earlier edits don't shift the positions of later edits
    all_edits.sort_by(|a, b| b.start.cmp(&a.start));

    // Apply edits
    let mut result = content.clone();
    for edit in &all_edits {
        // Validate range is within bounds
        if edit.end > result.len() {
            tracing::warn!(
                file = %file_fix.path.display(),
                start = edit.start,
                end = edit.end,
                len = result.len(),
                "Edit range out of bounds, skipping"
            );
            continue;
        }

        // Apply the edit
        result = format!(
            "{}{}{}",
            &result[..edit.start],
            edit.new_text,
            &result[edit.end..]
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
        OutputFormat::Json | OutputFormat::Github => {
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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create FileRelativeEdit for tests
    fn make_edit(start: usize, end: usize, new_text: &str) -> FileRelativeEdit {
        FileRelativeEdit {
            start,
            end,
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn test_file_fix_struct() {
        let fix = FileFix {
            path: PathBuf::from("/home/user/query.graphql"),
            diagnostics: vec![],
        };
        assert_eq!(fix.path.to_string_lossy(), "/home/user/query.graphql");
        assert!(fix.diagnostics.is_empty());
    }

    #[test]
    fn test_file_relative_edit_fields() {
        let edit = make_edit(10, 20, "replacement");
        assert_eq!(edit.start, 10);
        assert_eq!(edit.end, 20);
        assert_eq!(edit.new_text, "replacement");
    }

    #[test]
    fn test_file_relative_edit_empty_replacement() {
        let edit = make_edit(5, 15, "");
        assert_eq!(edit.start, 5);
        assert_eq!(edit.end, 15);
        assert!(edit.new_text.is_empty());
    }

    #[test]
    fn test_apply_edit_to_content() {
        // Test the edit application logic
        let content = "query { user { id name } }";
        let edit = make_edit(15, 17, "firstName");

        // Manually apply the edit to verify the logic
        let result = format!(
            "{}{}{}",
            &content[..edit.start],
            edit.new_text,
            &content[edit.end..]
        );

        assert_eq!(result, "query { user { firstName name } }");
    }

    #[test]
    fn test_apply_multiple_edits_reverse_order() {
        // Edits should be applied from end to start
        let content = "query { a b c }";

        // Replace 'c' (position 12-13)
        let replace_c = make_edit(12, 13, "third");
        // Replace 'a' (position 8-9)
        let replace_a = make_edit(8, 9, "first");

        // Apply in reverse order (end to start)
        let mut all_edits = vec![replace_c, replace_a];
        all_edits.sort_by(|a, b| b.start.cmp(&a.start));

        let mut result = content.to_string();
        for edit in all_edits {
            if edit.end <= result.len() {
                result = format!(
                    "{}{}{}",
                    &result[..edit.start],
                    edit.new_text,
                    &result[edit.end..]
                );
            }
        }

        assert_eq!(result, "query { first b third }");
    }

    #[test]
    fn test_edit_at_start_of_content() {
        let content = "query { user }";
        let edit = make_edit(0, 5, "mutation");

        let result = format!(
            "{}{}{}",
            &content[..edit.start],
            edit.new_text,
            &content[edit.end..]
        );

        assert_eq!(result, "mutation { user }");
    }

    #[test]
    fn test_edit_at_end_of_content() {
        let content = "query { user }";
        let edit = make_edit(14, 14, " extra");

        let result = format!(
            "{}{}{}",
            &content[..edit.start],
            edit.new_text,
            &content[edit.end..]
        );

        assert_eq!(result, "query { user } extra");
    }
}
