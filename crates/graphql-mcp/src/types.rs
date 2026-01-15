//! MCP tool parameter and result types
//!
//! These types define the interface between AI agents and the GraphQL tooling.
//! They are designed to be ergonomic for agents while providing rich information.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for the validate_document tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidateDocumentParams {
    /// The GraphQL document content to validate
    pub document: String,

    /// Optional file path for better error messages
    /// If not provided, defaults to "document.graphql"
    #[serde(default)]
    pub file_path: Option<String>,
}

/// Result of validating a GraphQL document
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidateDocumentResult {
    /// Whether the document is valid (no errors)
    pub valid: bool,

    /// Total number of errors found
    pub error_count: usize,

    /// Total number of warnings found
    pub warning_count: usize,

    /// List of diagnostics (errors and warnings)
    pub diagnostics: Vec<DiagnosticInfo>,
}

/// Parameters for the lint_document tool
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LintDocumentParams {
    /// The GraphQL document content to lint
    pub document: String,

    /// Optional file path for better error messages
    #[serde(default)]
    pub file_path: Option<String>,

    /// Whether to include auto-fix suggestions
    #[serde(default)]
    pub include_fixes: bool,
}

/// Result of linting a GraphQL document
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LintResult {
    /// Total number of lint issues found
    pub issue_count: usize,

    /// Number of issues that have auto-fixes available
    pub fixable_count: usize,

    /// List of lint diagnostics
    pub diagnostics: Vec<DiagnosticInfo>,
}

/// Result from validating multiple files
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileValidationResult {
    /// File path
    pub file: String,

    /// Whether the file is valid
    pub valid: bool,

    /// Diagnostics for this file
    pub diagnostics: Vec<DiagnosticInfo>,
}

/// A diagnostic message (error, warning, or info)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiagnosticInfo {
    /// Severity level
    pub severity: DiagnosticSeverity,

    /// Human-readable message
    pub message: String,

    /// Location in the source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<RangeInfo>,

    /// The rule that generated this diagnostic (for lint diagnostics)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,

    /// Auto-fix suggestion if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<FixSuggestion>,
}

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Error - document is invalid
    Error,
    /// Warning - potential issue
    Warning,
    /// Info - informational message
    Info,
    /// Hint - suggestion for improvement
    Hint,
}

/// A range in the source document
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RangeInfo {
    /// Start position
    pub start: LocationInfo,
    /// End position
    pub end: LocationInfo,
}

/// A position in the source document
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocationInfo {
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed)
    pub character: u32,
}

/// An auto-fix suggestion
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FixSuggestion {
    /// Description of what the fix does
    pub description: String,

    /// The text edits to apply
    pub edits: Vec<TextEditInfo>,
}

/// A text edit to apply
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextEditInfo {
    /// Range to replace
    pub range: RangeInfo,

    /// New text to insert
    pub new_text: String,
}

// Conversion implementations

impl From<graphql_ide::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(severity: graphql_ide::DiagnosticSeverity) -> Self {
        match severity {
            graphql_ide::DiagnosticSeverity::Error => DiagnosticSeverity::Error,
            graphql_ide::DiagnosticSeverity::Warning => DiagnosticSeverity::Warning,
            graphql_ide::DiagnosticSeverity::Information => DiagnosticSeverity::Info,
            graphql_ide::DiagnosticSeverity::Hint => DiagnosticSeverity::Hint,
        }
    }
}

impl From<graphql_ide::Range> for RangeInfo {
    fn from(range: graphql_ide::Range) -> Self {
        RangeInfo {
            start: LocationInfo {
                line: range.start.line,
                character: range.start.character,
            },
            end: LocationInfo {
                line: range.end.line,
                character: range.end.character,
            },
        }
    }
}

impl From<graphql_ide::Diagnostic> for DiagnosticInfo {
    fn from(diag: graphql_ide::Diagnostic) -> Self {
        DiagnosticInfo {
            severity: diag.severity.into(),
            message: diag.message,
            range: Some(diag.range.into()),
            rule: None,
            fix: None,
        }
    }
}
