use serde::{Deserialize, Serialize};

/// Diagnostic severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Position in a document (0-indexed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

/// Range in a document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Related information for a diagnostic
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedInfo {
    pub message: String,
    pub location: Location,
}

/// Location in a file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// A diagnostic message (error, warning, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Severity level
    pub severity: Severity,

    /// Range where the diagnostic applies
    pub range: Range,

    /// Diagnostic message
    pub message: String,

    /// Optional diagnostic code
    pub code: Option<String>,

    /// Source of the diagnostic (e.g., "graphql-validator")
    pub source: String,

    /// Related information
    pub related_info: Vec<RelatedInfo>,
}

impl Diagnostic {
    pub fn error(range: Range, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            range,
            message: message.into(),
            code: None,
            source: "graphql-project".to_string(),
            related_info: Vec::new(),
        }
    }

    pub fn warning(range: Range, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            range,
            message: message.into(),
            code: None,
            source: "graphql-project".to_string(),
            related_info: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    #[must_use]
    pub fn with_related_info(mut self, info: RelatedInfo) -> Self {
        self.related_info.push(info);
        self
    }
}

/// Convert apollo-compiler `DiagnosticList` to our `Diagnostic` type
///
/// This filters out fragment-related warnings for fragment-only documents.
#[must_use]
pub fn convert_apollo_diagnostics(
    compiler_diags: &crate::DiagnosticList,
    is_fragment_only: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for diag in compiler_diags.iter() {
        let message = diag.error.to_string();
        let message_lower = message.to_lowercase();

        // Skip "unused fragment" and "must be used" errors for fragment-only documents
        if is_fragment_only
            && (message_lower.contains("unused")
                || message_lower.contains("never used")
                || message_lower.contains("must be used"))
        {
            continue;
        }

        // Skip ALL "unused fragment" errors from apollo-compiler
        // We handle unused fragment warnings separately via linting
        if message_lower.contains("fragment")
            && (message_lower.contains("unused")
                || message_lower.contains("never used")
                || message_lower.contains("must be used"))
        {
            continue;
        }

        if let Some(loc_range) = diag.line_column_range() {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        // apollo-compiler uses 1-based, we use 0-based
                        line: loc_range.start.line.saturating_sub(1),
                        character: loc_range.start.column.saturating_sub(1),
                    },
                    end: Position {
                        line: loc_range.end.line.saturating_sub(1),
                        character: loc_range.end.column.saturating_sub(1),
                    },
                },
                severity: Severity::Error,
                code: None,
                source: "graphql".to_string(),
                message,
                related_info: Vec::new(),
            });
        }
    }

    diagnostics
}
