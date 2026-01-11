/// A text edit representing a change to apply to fix a lint issue
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Byte offset range in the file (or block for TS/JS files)
    pub offset_range: OffsetRange,
    /// The text to replace the range with (empty string means deletion)
    pub new_text: String,
}

impl TextEdit {
    /// Create a new text edit
    #[must_use]
    pub fn new(start: usize, end: usize, new_text: impl Into<String>) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            new_text: new_text.into(),
        }
    }

    /// Create a deletion edit (replace range with empty string)
    #[must_use]
    pub fn delete(start: usize, end: usize) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            new_text: String::new(),
        }
    }

    /// Create an insertion edit (insert text at position)
    #[must_use]
    pub fn insert(position: usize, text: impl Into<String>) -> Self {
        Self {
            offset_range: OffsetRange::at(position),
            new_text: text.into(),
        }
    }
}

/// A code fix that can be applied to resolve a lint diagnostic
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFix {
    /// Human-readable description of what the fix does
    pub label: String,
    /// The text edits to apply (in order)
    pub edits: Vec<TextEdit>,
}

impl CodeFix {
    /// Create a new code fix
    #[must_use]
    pub fn new(label: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            label: label.into(),
            edits,
        }
    }

    /// Create a simple deletion fix
    #[must_use]
    pub fn delete(label: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            label: label.into(),
            edits: vec![TextEdit::delete(start, end)],
        }
    }
}

/// Lint-specific diagnostic with byte offsets (not line/column)
/// This makes it compatible with Salsa and avoids premature position conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintDiagnostic {
    /// Byte offset range in the file (or block for TS/JS files)
    pub offset_range: OffsetRange,
    /// Severity (from rule default or config override)
    pub severity: LintSeverity,
    /// Human-readable message
    pub message: String,
    /// Rule identifier (e.g., `"deprecated_field"`)
    pub rule: String,
    /// For TS/JS files: line offset where the GraphQL block starts (0-based)
    /// This is used to adjust the final line position when converting to Diagnostic
    pub block_line_offset: Option<usize>,
    /// For TS/JS files: the GraphQL block source (for building `LineIndex`)
    /// When set, `offset_range` is relative to this source, not the full file
    pub block_source: Option<std::sync::Arc<str>>,
    /// Optional auto-fix for this diagnostic
    pub fix: Option<CodeFix>,
}

impl LintDiagnostic {
    /// Create a new lint diagnostic
    #[must_use]
    pub const fn new(
        offset_range: OffsetRange,
        severity: LintSeverity,
        message: String,
        rule: String,
    ) -> Self {
        Self {
            offset_range,
            severity,
            message,
            rule,
            block_line_offset: None,
            block_source: None,
            fix: None,
        }
    }

    /// Create a warning diagnostic
    #[must_use]
    pub fn warning(
        start: usize,
        end: usize,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            severity: LintSeverity::Warning,
            message: message.into(),
            rule: rule.into(),
            block_line_offset: None,
            block_source: None,
            fix: None,
        }
    }

    /// Create an error diagnostic
    #[must_use]
    pub fn error(
        start: usize,
        end: usize,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            severity: LintSeverity::Error,
            message: message.into(),
            rule: rule.into(),
            block_line_offset: None,
            block_source: None,
            fix: None,
        }
    }

    /// Create an info diagnostic
    #[must_use]
    pub fn info(
        start: usize,
        end: usize,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            severity: LintSeverity::Info,
            message: message.into(),
            rule: rule.into(),
            block_line_offset: None,
            block_source: None,
            fix: None,
        }
    }

    /// Set the block context for TS/JS files
    /// This allows proper position calculation when the diagnostic is from an extracted block
    #[must_use]
    pub fn with_block_context(mut self, line_offset: usize, source: std::sync::Arc<str>) -> Self {
        self.block_line_offset = Some(line_offset);
        self.block_source = Some(source);
        self
    }

    /// Add an auto-fix to this diagnostic
    #[must_use]
    pub fn with_fix(mut self, fix: CodeFix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Returns true if this diagnostic has an auto-fix available
    #[must_use]
    pub const fn has_fix(&self) -> bool {
        self.fix.is_some()
    }
}

/// Byte offset range in a file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OffsetRange {
    pub start: usize,
    pub end: usize,
}

impl OffsetRange {
    /// Create a new offset range
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Create a zero-width range at an offset
    #[must_use]
    pub const fn at(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }
}

/// Lint severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_range_creation() {
        let range = OffsetRange::new(10, 20);
        assert_eq!(range.start, 10);
        assert_eq!(range.end, 20);
    }

    #[test]
    fn test_offset_range_at() {
        let range = OffsetRange::at(15);
        assert_eq!(range.start, 15);
        assert_eq!(range.end, 15);
    }

    #[test]
    fn test_lint_diagnostic_warning() {
        let diag = LintDiagnostic::warning(5, 10, "Test warning", "test_rule");
        assert_eq!(diag.severity, LintSeverity::Warning);
        assert_eq!(diag.message, "Test warning");
        assert_eq!(diag.rule, "test_rule");
        assert_eq!(diag.offset_range.start, 5);
        assert_eq!(diag.offset_range.end, 10);
        assert!(!diag.has_fix());
    }

    #[test]
    fn test_text_edit_creation() {
        let edit = TextEdit::new(10, 20, "replacement");
        assert_eq!(edit.offset_range.start, 10);
        assert_eq!(edit.offset_range.end, 20);
        assert_eq!(edit.new_text, "replacement");
    }

    #[test]
    fn test_text_edit_delete() {
        let edit = TextEdit::delete(5, 15);
        assert_eq!(edit.offset_range.start, 5);
        assert_eq!(edit.offset_range.end, 15);
        assert_eq!(edit.new_text, "");
    }

    #[test]
    fn test_text_edit_insert() {
        let edit = TextEdit::insert(10, "inserted text");
        assert_eq!(edit.offset_range.start, 10);
        assert_eq!(edit.offset_range.end, 10);
        assert_eq!(edit.new_text, "inserted text");
    }

    #[test]
    fn test_code_fix_creation() {
        let fix = CodeFix::new("Remove unused variable", vec![TextEdit::delete(10, 20)]);
        assert_eq!(fix.label, "Remove unused variable");
        assert_eq!(fix.edits.len(), 1);
    }

    #[test]
    fn test_code_fix_delete() {
        let fix = CodeFix::delete("Remove variable", 10, 20);
        assert_eq!(fix.label, "Remove variable");
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].new_text, "");
    }

    #[test]
    fn test_diagnostic_with_fix() {
        let diag = LintDiagnostic::warning(5, 10, "Test warning", "test_rule")
            .with_fix(CodeFix::delete("Fix it", 5, 10));
        assert!(diag.has_fix());
        assert_eq!(diag.fix.unwrap().label, "Fix it");
    }
}
