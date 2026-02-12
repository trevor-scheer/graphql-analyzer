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

/// Lint-specific diagnostic with byte offsets (not line/column).
///
/// The `span` field carries both the byte offset range and block context
/// (for embedded GraphQL in TS/JS), ensuring correct position mapping.
/// Use [`DocumentRef::span()`](graphql_syntax::DocumentRef::span) to create spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintDiagnostic {
    /// Source span with byte offsets and block context for position mapping
    pub span: graphql_syntax::SourceSpan,
    /// Severity (from rule default or config override)
    pub severity: LintSeverity,
    /// Human-readable message
    pub message: String,
    /// Rule identifier (e.g., `"deprecated_field"`)
    pub rule: String,
    /// Optional auto-fix for this diagnostic
    pub fix: Option<CodeFix>,
}

impl LintDiagnostic {
    /// Create a new lint diagnostic
    #[must_use]
    pub fn new(
        span: graphql_syntax::SourceSpan,
        severity: LintSeverity,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            span,
            severity,
            message: message.into(),
            rule: rule.into(),
            fix: None,
        }
    }

    /// Create a warning diagnostic
    #[must_use]
    pub fn warning(
        span: graphql_syntax::SourceSpan,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            span,
            severity: LintSeverity::Warning,
            message: message.into(),
            rule: rule.into(),
            fix: None,
        }
    }

    /// Create an error diagnostic
    #[must_use]
    pub fn error(
        span: graphql_syntax::SourceSpan,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            span,
            severity: LintSeverity::Error,
            message: message.into(),
            rule: rule.into(),
            fix: None,
        }
    }

    /// Create an info diagnostic
    #[must_use]
    pub fn info(
        span: graphql_syntax::SourceSpan,
        message: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            span,
            severity: LintSeverity::Info,
            message: message.into(),
            rule: rule.into(),
            fix: None,
        }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OffsetRange {
    pub start: usize,
    pub end: usize,
}

impl std::fmt::Display for OffsetRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
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

impl std::fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
        }
    }
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
        let span = graphql_syntax::SourceSpan::default();
        let diag = LintDiagnostic::warning(
            graphql_syntax::SourceSpan {
                start: 5,
                end: 10,
                ..span
            },
            "Test warning",
            "test_rule",
        );
        assert_eq!(diag.severity, LintSeverity::Warning);
        assert_eq!(diag.message, "Test warning");
        assert_eq!(diag.rule, "test_rule");
        assert_eq!(diag.span.start, 5);
        assert_eq!(diag.span.end, 10);
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
        let span = graphql_syntax::SourceSpan {
            start: 5,
            end: 10,
            ..Default::default()
        };
        let diag = LintDiagnostic::warning(span, "Test warning", "test_rule")
            .with_fix(CodeFix::delete("Fix it", 5, 10));
        assert!(diag.has_fix());
        assert_eq!(diag.fix.unwrap().label, "Fix it");
    }
}
