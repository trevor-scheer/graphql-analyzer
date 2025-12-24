/// Lint-specific diagnostic with byte offsets (not line/column)
/// This makes it compatible with Salsa and avoids premature position conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintDiagnostic {
    /// Byte offset range in the file
    pub offset_range: OffsetRange,
    /// Severity (from rule default or config override)
    pub severity: LintSeverity,
    /// Human-readable message
    pub message: String,
    /// Rule identifier (e.g., `"deprecated_field"`)
    pub rule: &'static str,
}

impl LintDiagnostic {
    /// Create a new lint diagnostic
    #[must_use]
    pub const fn new(
        offset_range: OffsetRange,
        severity: LintSeverity,
        message: String,
        rule: &'static str,
    ) -> Self {
        Self {
            offset_range,
            severity,
            message,
            rule,
        }
    }

    /// Create a warning diagnostic
    #[must_use]
    pub fn warning(
        start: usize,
        end: usize,
        message: impl Into<String>,
        rule: &'static str,
    ) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            severity: LintSeverity::Warning,
            message: message.into(),
            rule,
        }
    }

    /// Create an error diagnostic
    #[must_use]
    pub fn error(start: usize, end: usize, message: impl Into<String>, rule: &'static str) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            severity: LintSeverity::Error,
            message: message.into(),
            rule,
        }
    }

    /// Create an info diagnostic
    #[must_use]
    pub fn info(start: usize, end: usize, message: impl Into<String>, rule: &'static str) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            severity: LintSeverity::Info,
            message: message.into(),
            rule,
        }
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
    }
}
