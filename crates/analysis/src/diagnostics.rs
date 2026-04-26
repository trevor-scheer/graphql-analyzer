// Diagnostic types for GraphQL analysis

use std::sync::Arc;

/// A tag attached to a diagnostic providing additional classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticTag {
    /// The diagnostic marks code as unnecessary (e.g., unused fragments)
    Unnecessary,
    /// The diagnostic marks code as deprecated
    Deprecated,
}

/// A text edit (line/column based) for an autofix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: DiagnosticRange,
    pub new_text: String,
}

/// An autofix attached to a diagnostic — the line/column-based equivalent of
/// `graphql_linter::CodeFix`. Carrying it on `Diagnostic` lets downstream
/// consumers (LSP code actions, `ESLint` shim) surface fixes uniformly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFix {
    pub label: String,
    pub edits: Vec<TextEdit>,
}

/// A diagnostic message (error, warning, or info)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity level
    pub severity: Severity,
    /// Human-readable message
    pub message: Arc<str>,
    /// Location in the file (line, column range)
    pub range: DiagnosticRange,
    /// Source of the diagnostic (e.g., "graphql-parser", "graphql-linter")
    pub source: Arc<str>,
    /// Optional diagnostic code
    pub code: Option<Arc<str>>,
    /// Optional ESLint-compatible messageId. Forwarded to `LintMessage.messageId`
    /// by the `ESLint` shim so drop-in users get the same per-diagnostic-site id
    /// graphql-eslint emits.
    pub message_id: Option<Arc<str>>,
    /// Optional autofix carried alongside the diagnostic.
    pub fix: Option<CodeFix>,
    /// Optional help text explaining how to resolve the issue
    pub help: Option<Arc<str>>,
    /// Optional documentation URL for the rule
    pub url: Option<Arc<str>>,
    /// Diagnostic tags for additional classification
    pub tags: Vec<DiagnosticTag>,
}

impl Diagnostic {
    /// Create an error diagnostic
    #[must_use]
    pub fn error(message: impl Into<Arc<str>>, range: DiagnosticRange) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            range,
            source: "graphql-analysis".into(),
            code: None,
            message_id: None,
            fix: None,
            help: None,
            url: None,
            tags: Vec::new(),
        }
    }

    /// Create a warning diagnostic
    #[must_use]
    pub fn warning(message: impl Into<Arc<str>>, range: DiagnosticRange) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            range,
            source: "graphql-analysis".into(),
            code: None,
            message_id: None,
            fix: None,
            help: None,
            url: None,
            tags: Vec::new(),
        }
    }

    /// Create an info diagnostic
    #[must_use]
    pub fn info(message: impl Into<Arc<str>>, range: DiagnosticRange) -> Self {
        Self {
            severity: Severity::Info,
            message: message.into(),
            range,
            source: "graphql-analysis".into(),
            code: None,
            message_id: None,
            fix: None,
            help: None,
            url: None,
            tags: Vec::new(),
        }
    }

    /// Create a diagnostic with a specific source and code
    #[must_use]
    pub fn with_source_and_code(
        severity: Severity,
        message: impl Into<Arc<str>>,
        range: DiagnosticRange,
        source: impl Into<Arc<str>>,
        code: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            severity,
            message: message.into(),
            range,
            source: source.into(),
            code: Some(code.into()),
            message_id: None,
            fix: None,
            help: None,
            url: None,
            tags: Vec::new(),
        }
    }
}

/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Location range for a diagnostic (line and column based)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DiagnosticRange {
    /// Start position (line, column) - 0-indexed
    pub start: Position,
    /// End position (line, column) - 0-indexed
    pub end: Position,
}

impl DiagnosticRange {
    /// Create a new range
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Create a zero-width range at a position
    #[must_use]
    pub const fn at(position: Position) -> Self {
        Self {
            start: position,
            end: position,
        }
    }
}

/// A position in a file (line and column)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed, UTF-8 byte offset)
    pub character: u32,
}

impl Position {
    /// Create a new position
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_error() {
        let diag = Diagnostic::error("Test error", DiagnosticRange::default());
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.message.as_ref(), "Test error");
        assert_eq!(diag.source.as_ref(), "graphql-analysis");
        assert!(diag.code.is_none());
    }

    #[test]
    fn test_diagnostic_warning() {
        let diag = Diagnostic::warning("Test warning", DiagnosticRange::default());
        assert_eq!(diag.severity, Severity::Warning);
    }

    #[test]
    fn test_diagnostic_with_code() {
        let diag = Diagnostic::with_source_and_code(
            Severity::Warning,
            "Test warning",
            DiagnosticRange::default(),
            "test-linter",
            "TEST001",
        );
        assert_eq!(diag.source.as_ref(), "test-linter");
        assert_eq!(diag.code.as_ref().unwrap().as_ref(), "TEST001");
    }

    #[test]
    fn test_position() {
        let pos = Position::new(10, 20);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 20);
    }

    #[test]
    fn test_range() {
        let start = Position::new(0, 0);
        let end = Position::new(0, 10);
        let range = DiagnosticRange::new(start, end);
        assert_eq!(range.start, start);
        assert_eq!(range.end, end);
    }

    #[test]
    fn test_range_at_position() {
        let pos = Position::new(5, 10);
        let range = DiagnosticRange::at(pos);
        assert_eq!(range.start, pos);
        assert_eq!(range.end, pos);
    }
}
