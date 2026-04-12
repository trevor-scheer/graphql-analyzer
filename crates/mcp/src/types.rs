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

    /// Optional project name to validate against
    /// If not provided, uses the first/only loaded project
    #[serde(default)]
    pub project: Option<String>,
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

/// Result from getting all project diagnostics
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectDiagnosticsResult {
    /// The project name (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,

    /// Total number of diagnostics across all files
    pub total_count: usize,

    /// Number of files with diagnostics
    pub file_count: usize,

    /// Diagnostics grouped by file (only files with diagnostics are included)
    pub files: Vec<FileDiagnostics>,
}

/// Diagnostics for a single file
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileDiagnostics {
    /// File path
    pub file: String,

    /// Diagnostics for this file
    pub diagnostics: Vec<DiagnosticInfo>,
}

/// Result of loading a project
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadProjectResult {
    /// Whether the project was loaded successfully
    pub success: bool,

    /// The project name
    pub project: String,

    /// Status message
    pub message: String,
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

/// Parameters for position-based tools (goto definition, hover, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilePositionParams {
    /// File path (absolute path or file:// URI) of a file in the loaded project
    #[schemars(description = "File path of a loaded project file (absolute path or file:// URI)")]
    pub file_path: String,

    /// Line number (0-indexed)
    #[schemars(description = "Line number (0-indexed)")]
    pub line: u32,

    /// Column/character number (0-indexed)
    #[schemars(description = "Column number (0-indexed)")]
    pub character: u32,

    /// Optional project name
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

/// Parameters for find_references tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindReferencesParams {
    /// File path (absolute path or file:// URI) of a file in the loaded project
    #[schemars(description = "File path of a loaded project file (absolute path or file:// URI)")]
    pub file_path: String,

    /// Line number (0-indexed)
    #[schemars(description = "Line number (0-indexed)")]
    pub line: u32,

    /// Column/character number (0-indexed)
    #[schemars(description = "Column number (0-indexed)")]
    pub character: u32,

    /// Whether to include the declaration itself in the results
    #[schemars(description = "Whether to include the declaration in results (default: true)")]
    #[serde(default = "default_true")]
    pub include_declaration: bool,

    /// Optional project name
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Parameters for document_symbols tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSymbolsParams {
    /// File path (absolute path or file:// URI) of a file in the loaded project
    #[schemars(description = "File path of a loaded project file (absolute path or file:// URI)")]
    pub file_path: String,

    /// Optional project name
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

/// Parameters for workspace_symbols tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolsParams {
    /// Search query to filter symbols
    #[schemars(description = "Search query to filter symbols (empty string returns all symbols)")]
    pub query: String,

    /// Optional project name
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

/// Parameters for get_file_diagnostics tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileDiagnosticsParams {
    /// File path (absolute path or file:// URI) of a file in the loaded project
    #[schemars(description = "File path of a loaded project file (absolute path or file:// URI)")]
    pub file_path: String,

    /// Optional project name
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

/// A location result (file + range)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocationResult {
    /// File path
    pub file: String,
    /// Range in the file
    pub range: RangeInfo,
}

/// Result of goto_definition or find_references
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocationsResult {
    /// The locations found
    pub locations: Vec<LocationResult>,
    /// Number of locations found
    pub count: usize,
}

/// Result of hover
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HoverResultInfo {
    /// Markdown-formatted hover contents
    pub contents: String,
    /// Optional range of the hovered element
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<RangeInfo>,
}

/// A symbol in a document or workspace
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Symbol kind (e.g., "type", "field", "query", "mutation", "fragment")
    pub kind: String,
    /// Optional detail (e.g., type signature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Range of the full symbol definition
    pub range: RangeInfo,
    /// Range of just the symbol name
    pub selection_range: RangeInfo,
    /// Child symbols (e.g., fields within a type)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SymbolInfo>,
}

/// A workspace symbol result (includes file location)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolInfo {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: String,
    /// File and range where the symbol is defined
    pub location: LocationResult,
    /// Optional container name (e.g., parent type for fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
}

/// Result of document_symbols
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSymbolsResult {
    /// Hierarchical list of symbols in the document
    pub symbols: Vec<SymbolInfo>,
    /// Total number of top-level symbols
    pub count: usize,
}

/// Result of workspace_symbols
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolsResult {
    /// Flat list of matching symbols
    pub symbols: Vec<WorkspaceSymbolInfo>,
    /// Number of symbols found
    pub count: usize,
}

/// A completion item
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionInfo {
    /// Display label
    pub label: String,
    /// Kind of completion (e.g., "field", "type", "fragment", "directive")
    pub kind: String,
    /// Optional detail (e.g., return type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Optional documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Whether this item is deprecated
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

/// Result of get_completions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionsResult {
    /// List of completion items
    pub items: Vec<CompletionInfo>,
    /// Number of completion items
    pub count: usize,
}

// Conversion implementations from graphql_ide types

impl From<graphql_ide::Location> for LocationResult {
    fn from(loc: graphql_ide::Location) -> Self {
        LocationResult {
            file: loc.file.as_str().to_string(),
            range: loc.range.into(),
        }
    }
}

fn symbol_kind_str(kind: graphql_ide::SymbolKind) -> &'static str {
    match kind {
        graphql_ide::SymbolKind::Type => "type",
        graphql_ide::SymbolKind::Field => "field",
        graphql_ide::SymbolKind::Query => "query",
        graphql_ide::SymbolKind::Mutation => "mutation",
        graphql_ide::SymbolKind::Subscription => "subscription",
        graphql_ide::SymbolKind::Fragment => "fragment",
        graphql_ide::SymbolKind::EnumValue => "enum_value",
        graphql_ide::SymbolKind::Scalar => "scalar",
        graphql_ide::SymbolKind::Input => "input",
        graphql_ide::SymbolKind::Interface => "interface",
        graphql_ide::SymbolKind::Union => "union",
        graphql_ide::SymbolKind::Enum => "enum",
        graphql_ide::SymbolKind::Directive => "directive",
    }
}

impl From<graphql_ide::DocumentSymbol> for SymbolInfo {
    fn from(sym: graphql_ide::DocumentSymbol) -> Self {
        SymbolInfo {
            name: sym.name,
            kind: symbol_kind_str(sym.kind).to_string(),
            detail: sym.detail,
            range: sym.range.into(),
            selection_range: sym.selection_range.into(),
            children: sym.children.into_iter().map(SymbolInfo::from).collect(),
        }
    }
}

impl From<graphql_ide::WorkspaceSymbol> for WorkspaceSymbolInfo {
    fn from(sym: graphql_ide::WorkspaceSymbol) -> Self {
        WorkspaceSymbolInfo {
            name: sym.name,
            kind: symbol_kind_str(sym.kind).to_string(),
            location: sym.location.into(),
            container_name: sym.container_name,
        }
    }
}

fn completion_kind_str(kind: graphql_ide::CompletionKind) -> &'static str {
    match kind {
        graphql_ide::CompletionKind::Field => "field",
        graphql_ide::CompletionKind::Type => "type",
        graphql_ide::CompletionKind::Fragment => "fragment",
        graphql_ide::CompletionKind::Directive => "directive",
        graphql_ide::CompletionKind::EnumValue => "enum_value",
        graphql_ide::CompletionKind::Argument => "argument",
        graphql_ide::CompletionKind::Variable => "variable",
        graphql_ide::CompletionKind::Keyword => "keyword",
    }
}

impl From<graphql_ide::CompletionItem> for CompletionInfo {
    fn from(item: graphql_ide::CompletionItem) -> Self {
        CompletionInfo {
            label: item.label,
            kind: completion_kind_str(item.kind).to_string(),
            detail: item.detail,
            documentation: item.documentation,
            deprecated: item.deprecated,
        }
    }
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
