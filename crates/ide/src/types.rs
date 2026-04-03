//! POD types for IDE features.
//!
//! This module contains Plain Old Data (POD) structs with public fields
//! that serve as the interface between the analysis layer and the LSP layer.

/// Position in a file (editor coordinates, 0-indexed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Range in a file (editor coordinates)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// A text edit representing a change to apply to fix an issue
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Range to replace
    pub range: Range,
    /// The text to replace the range with (empty string means deletion)
    pub new_text: String,
}

impl TextEdit {
    /// Create a new text edit
    #[must_use]
    pub fn new(range: Range, new_text: impl Into<String>) -> Self {
        Self {
            range,
            new_text: new_text.into(),
        }
    }
}

/// A code fix that can be applied to resolve a diagnostic
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFix {
    /// Human-readable description of what the fix does
    pub label: String,
    /// The text edits to apply
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
}

/// File path (stored as URI internally)
///
/// All files are stored and looked up using URIs for consistency.
/// Use `from_path` to convert filesystem paths to proper file:// URIs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub String);

impl FilePath {
    /// Create a `FilePath` from a string that is already a URI
    pub fn new(uri: impl Into<String>) -> Self {
        Self(uri.into())
    }

    /// Create a `FilePath` from a filesystem path, converting to file:// URI
    ///
    /// This handles:
    /// - Already-URI strings (file://, schema://, https://) - passed through unchanged
    /// - Unix absolute paths (/home/user/file.graphql) - converted to file:// URI
    /// - Other paths - prefixed with file:///
    #[must_use]
    pub fn from_path(path: &std::path::Path) -> Self {
        let path_str = path.to_string_lossy();

        // Already a URI - pass through
        if path_str.starts_with("file://") || path_str.contains("://") {
            return Self(path_str.to_string());
        }

        // Unix absolute path
        if path_str.starts_with('/') {
            return Self(format!("file://{path_str}"));
        }

        // Windows or relative path
        Self(format!("file:///{path_str}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for FilePath {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FilePath {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Location in a specific file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: FilePath,
    pub range: Range,
}

impl Location {
    #[must_use]
    pub const fn new(file: FilePath, range: Range) -> Self {
        Self { file, range }
    }
}

/// Completion item kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Field,
    Type,
    Fragment,
    Directive,
    EnumValue,
    Argument,
    Variable,
    Keyword,
}

/// Insert text format for completion items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertTextFormat {
    PlainText,
    Snippet,
}

/// Completion item
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    pub insert_text_format: Option<InsertTextFormat>,
    pub sort_text: Option<String>,
    pub deprecated: bool,
}

impl CompletionItem {
    pub fn new(label: impl Into<String>, kind: CompletionKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            documentation: None,
            insert_text: None,
            insert_text_format: None,
            sort_text: None,
            deprecated: false,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn with_documentation(mut self, doc: impl Into<String>) -> Self {
        self.documentation = Some(doc.into());
        self
    }

    #[must_use]
    pub fn with_insert_text(mut self, text: impl Into<String>) -> Self {
        self.insert_text = Some(text.into());
        self
    }

    #[must_use]
    pub const fn with_insert_text_format(mut self, format: InsertTextFormat) -> Self {
        self.insert_text_format = Some(format);
        self
    }

    #[must_use]
    pub fn with_sort_text(mut self, sort_text: impl Into<String>) -> Self {
        self.sort_text = Some(sort_text.into());
        self
    }

    #[must_use]
    pub const fn with_deprecated(mut self, deprecated: bool) -> Self {
        self.deprecated = deprecated;
        self
    }
}

/// Hover information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverResult {
    /// Markdown content
    pub contents: String,
    /// Optional range for the hover
    pub range: Option<Range>,
}

impl HoverResult {
    pub fn new(contents: impl Into<String>) -> Self {
        Self {
            contents: contents.into(),
            range: None,
        }
    }

    #[must_use]
    pub const fn with_range(mut self, range: Range) -> Self {
        self.range = Some(range);
        self
    }
}

/// Diagnostic severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Diagnostic (error, warning, hint)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub code: Option<String>,
    pub source: String,
    /// Optional auto-fix for this diagnostic
    pub fix: Option<CodeFix>,
}

impl Diagnostic {
    pub fn new(
        range: Range,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            range,
            severity,
            message: message.into(),
            code: None,
            source: source.into(),
            fix: None,
        }
    }

    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    #[must_use]
    pub fn with_fix(mut self, fix: CodeFix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// Kind of GraphQL symbol for document/workspace symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Object type definition
    Type,
    /// Field definition
    Field,
    /// Query operation
    Query,
    /// Mutation operation
    Mutation,
    /// Subscription operation
    Subscription,
    /// Fragment definition
    Fragment,
    /// Enum value
    EnumValue,
    /// Scalar type
    Scalar,
    /// Input type
    Input,
    /// Interface type
    Interface,
    /// Union type
    Union,
    /// Enum type
    Enum,
}

/// A document symbol (hierarchical structure for outline view)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Optional detail (e.g., type signature)
    pub detail: Option<String>,
    /// Full range of the symbol (entire definition)
    pub range: Range,
    /// Selection range (just the name)
    pub selection_range: Range,
    /// Child symbols (e.g., fields within a type)
    pub children: Vec<DocumentSymbol>,
}

impl DocumentSymbol {
    pub fn new(
        name: impl Into<String>,
        kind: SymbolKind,
        range: Range,
        selection_range: Range,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            detail: None,
            range,
            selection_range,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn with_children(mut self, children: Vec<Self>) -> Self {
        self.children = children;
        self
    }
}

/// A workspace symbol (flat structure for global search)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Location of the symbol
    pub location: Location,
    /// Container name (e.g., parent type for fields)
    pub container_name: Option<String>,
}

impl WorkspaceSymbol {
    pub fn new(name: impl Into<String>, kind: SymbolKind, location: Location) -> Self {
        Self {
            name: name.into(),
            kind,
            location,
            container_name: None,
        }
    }

    #[must_use]
    pub fn with_container(mut self, container: impl Into<String>) -> Self {
        self.container_name = Some(container.into());
        self
    }
}

/// Code lens information for a deprecated field
///
/// Used to show usage counts for deprecated fields in schema files.
/// The code lens appears above the field definition and shows how many
/// usages exist across the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLensInfo {
    /// Range where the code lens should appear (field definition range)
    pub range: Range,
    /// Number of usages of this deprecated field
    pub usage_count: usize,
    /// The type name that contains the deprecated field
    pub type_name: String,
    /// The deprecated field name
    pub field_name: String,
    /// Optional deprecation reason
    pub deprecation_reason: Option<String>,
    /// Locations of all usages (for navigation)
    pub usage_locations: Vec<Location>,
}

impl CodeLensInfo {
    /// Create a new code lens for a deprecated field
    #[must_use]
    pub fn new(
        range: Range,
        type_name: impl Into<String>,
        field_name: impl Into<String>,
        usage_count: usize,
        usage_locations: Vec<Location>,
    ) -> Self {
        Self {
            range,
            usage_count,
            type_name: type_name.into(),
            field_name: field_name.into(),
            deprecation_reason: None,
            usage_locations,
        }
    }

    /// Add a deprecation reason
    #[must_use]
    pub fn with_deprecation_reason(mut self, reason: impl Into<String>) -> Self {
        self.deprecation_reason = Some(reason.into());
        self
    }
}

/// Statistics about schema types
#[derive(Debug, Clone, Default)]
pub struct SchemaStats {
    /// Number of object types
    pub objects: usize,
    /// Number of interface types
    pub interfaces: usize,
    /// Number of union types
    pub unions: usize,
    /// Number of enum types
    pub enums: usize,
    /// Number of scalar types
    pub scalars: usize,
    /// Number of input object types
    pub input_objects: usize,
    /// Total number of fields across all types
    pub total_fields: usize,
    /// Number of directive definitions
    pub directives: usize,
}

impl SchemaStats {
    /// Total number of types (all kinds)
    #[must_use]
    pub fn total_types(&self) -> usize {
        self.objects
            + self.interfaces
            + self.unions
            + self.enums
            + self.scalars
            + self.input_objects
    }
}

/// Kind of folding range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldingRangeKind {
    /// Folding range for a region (selection sets, definitions)
    Region,
    /// Folding range for a comment
    Comment,
}

/// A folding range in a document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingRange {
    /// The zero-based line number from where the folded range starts
    pub start_line: u32,
    /// The zero-based line number where the folded range ends
    pub end_line: u32,
    /// Describes the kind of the folding range
    pub kind: FoldingRangeKind,
}

impl FoldingRange {
    /// Create a new folding range
    #[must_use]
    pub const fn new(start_line: u32, end_line: u32, kind: FoldingRangeKind) -> Self {
        Self {
            start_line,
            end_line,
            kind,
        }
    }
}

/// A reference to a fragment spread
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentReference {
    /// Location of the fragment spread
    pub location: Location,
}

impl FragmentReference {
    #[must_use]
    pub const fn new(location: Location) -> Self {
        Self { location }
    }
}

/// Fragment usage analysis result
///
/// Contains information about how a fragment is used across the project,
/// including its definition location, all usages, and transitive dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentUsage {
    /// Fragment name
    pub name: String,
    /// File where the fragment is defined
    pub definition_file: FilePath,
    /// Range of the fragment definition (just the name)
    pub definition_range: Range,
    /// All locations where this fragment is spread
    pub usages: Vec<FragmentReference>,
    /// Names of other fragments this fragment depends on (transitively)
    pub transitive_dependencies: Vec<String>,
}

impl FragmentUsage {
    /// Get the number of usages (excluding the definition)
    #[must_use]
    pub fn usage_count(&self) -> usize {
        self.usages.len()
    }

    /// Check if this fragment is unused (has no references)
    #[must_use]
    pub fn is_unused(&self) -> bool {
        self.usages.is_empty()
    }
}

/// Code lens information for displaying actionable info above definitions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLens {
    /// Range where the code lens should be displayed
    pub range: Range,
    /// Title to display (e.g., "5 references")
    pub title: String,
    /// Optional command to execute when clicked
    pub command: Option<CodeLensCommand>,
}

impl CodeLens {
    pub fn new(range: Range, title: impl Into<String>) -> Self {
        Self {
            range,
            title: title.into(),
            command: None,
        }
    }

    #[must_use]
    pub fn with_command(mut self, command: CodeLensCommand) -> Self {
        self.command = Some(command);
        self
    }
}

/// Command associated with a code lens
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLensCommand {
    /// Command identifier
    pub command: String,
    /// Human-readable title
    pub title: String,
    /// Optional arguments for the command
    pub arguments: Vec<String>,
}

impl CodeLensCommand {
    pub fn new(command: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            title: title.into(),
            arguments: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_arguments(mut self, args: Vec<String>) -> Self {
        self.arguments = args;
        self
    }
}

/// Kind of inlay hint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlayHintKind {
    /// Type hint (e.g., showing return type of a field)
    Type,
    /// Parameter hint (e.g., showing parameter name)
    Parameter,
}

/// An inlay hint that shows inline type information without modifying source
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHint {
    /// Position where the hint should be displayed (after the element)
    pub position: Position,
    /// The hint label text
    pub label: String,
    /// Kind of inlay hint
    pub kind: InlayHintKind,
    /// Whether to add padding before the hint
    pub padding_left: bool,
    /// Whether to add padding after the hint
    pub padding_right: bool,
}

impl InlayHint {
    /// Create a new inlay hint
    #[must_use]
    pub fn new(position: Position, label: impl Into<String>, kind: InlayHintKind) -> Self {
        Self {
            position,
            label: label.into(),
            kind,
            padding_left: true,
            padding_right: false,
        }
    }

    /// Set padding options
    #[must_use]
    pub const fn with_padding(mut self, left: bool, right: bool) -> Self {
        self.padding_left = left;
        self.padding_right = right;
        self
    }
}

/// Result of loading schemas from configuration.
///
/// This type captures both the successfully loaded local schemas and any
/// remote introspection configurations that require async fetching.
#[derive(Debug, Clone, Default)]
pub struct SchemaLoadResult {
    /// Number of schema files successfully loaded (includes Apollo client builtins)
    pub loaded_count: usize,
    /// Paths of loaded schema files (for diagnostics collection)
    pub loaded_paths: Vec<std::path::PathBuf>,
    /// Pending introspection configurations that need async fetching.
    /// These require network access and should be handled asynchronously
    /// by the calling layer (e.g., LSP server).
    pub pending_introspections: Vec<PendingIntrospection>,
    /// Content mismatch errors found during schema loading.
    /// These indicate files that contain executable definitions
    /// (operations/fragments) instead of schema definitions.
    pub content_errors: Vec<SchemaContentError>,
    /// Schema patterns that matched no files on disk.
    /// Each entry is the original pattern string from the config.
    pub unmatched_patterns: Vec<String>,
}

impl SchemaLoadResult {
    /// Returns true if no user schema files were loaded and no remote
    /// introspection is pending. The Apollo Client builtins are always
    /// loaded and don't count as user schemas.
    #[must_use]
    pub fn has_no_user_schema(&self) -> bool {
        self.loaded_paths.is_empty() && self.pending_introspections.is_empty()
    }
}

/// Result of loading documents from configuration.
///
/// This type captures both the successfully loaded document files and any
/// patterns that matched no files on disk.
#[derive(Debug, Clone, Default)]
pub struct DocumentLoadResult {
    /// Number of document files successfully loaded
    pub loaded_count: usize,
    /// Document patterns that matched no files on disk.
    /// Each entry is the original pattern string from the config.
    pub unmatched_patterns: Vec<String>,
}

impl DocumentLoadResult {
    /// Returns true if no document files were loaded.
    #[must_use]
    pub fn has_no_documents(&self) -> bool {
        self.loaded_count == 0
    }
}

/// A content mismatch error found during schema loading.
#[derive(Debug, Clone)]
pub struct SchemaContentError {
    /// The pattern that matched this file
    pub pattern: String,
    /// Path to the file with mismatched content
    pub file_path: std::path::PathBuf,
    /// Names of executable definitions found (shouldn't be in schema files)
    pub unexpected_definitions: Vec<String>,
}

/// A pending remote schema introspection request.
///
/// This represents a remote GraphQL endpoint that should be introspected
/// to fetch its schema. The caller is responsible for performing the async
/// introspection and registering the resulting SDL as a virtual file.
#[derive(Debug, Clone)]
pub struct PendingIntrospection {
    /// The GraphQL endpoint URL to introspect
    pub url: String,
    /// HTTP headers to include in the introspection request (e.g., for authentication)
    pub headers: Option<std::collections::HashMap<String, String>>,
    /// Request timeout in seconds (default: 30)
    pub timeout: Option<u64>,
    /// Number of retry attempts on failure (default: 0)
    pub retry: Option<u32>,
}

impl PendingIntrospection {
    /// Create a new pending introspection from a config
    #[must_use]
    pub fn from_config(config: &graphql_config::IntrospectionSchemaConfig) -> Self {
        Self {
            url: config.url.clone(),
            headers: config.headers.clone(),
            timeout: config.timeout,
            retry: config.retry,
        }
    }

    /// Generate a virtual file URI for this introspection endpoint.
    /// Uses the format `schema://<host>/<path>/schema.graphql` to uniquely identify
    /// remote schemas. The `.graphql` extension ensures proper syntax highlighting.
    #[must_use]
    pub fn virtual_uri(&self) -> String {
        format!(
            "schema://{}/schema.graphql",
            self.url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
        )
    }
}

/// Status information for a project.
///
/// Used by the LSP status command to report project health and metrics.
#[derive(Debug, Clone, Default)]
pub struct ProjectStatus {
    /// Number of schema files loaded
    pub schema_file_count: usize,
    /// Number of document files loaded
    pub document_file_count: usize,
}

impl ProjectStatus {
    /// Create a new project status
    #[must_use]
    pub const fn new(schema_file_count: usize, document_file_count: usize) -> Self {
        Self {
            schema_file_count,
            document_file_count,
        }
    }

    /// Total number of files (schema + documents)
    #[must_use]
    pub const fn total_files(&self) -> usize {
        self.schema_file_count + self.document_file_count
    }

    /// Whether a schema has been loaded (at least one schema file)
    #[must_use]
    pub const fn has_schema(&self) -> bool {
        self.schema_file_count > 0
    }
}

/// Result of a rename operation.
///
/// Contains a map of file paths to the text edits needed in each file
/// to perform the rename atomically across the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameResult {
    pub changes: std::collections::HashMap<FilePath, Vec<TextEdit>>,
}

impl RenameResult {
    #[must_use]
    pub fn new(changes: std::collections::HashMap<FilePath, Vec<TextEdit>>) -> Self {
        Self { changes }
    }
}

/// Signature help result for displaying argument information.
///
/// Shows the signature of a field or directive when the cursor is inside
/// an argument list, highlighting the active parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: Option<u32>,
    pub active_parameter: Option<u32>,
}

/// Information about a single signature (field or directive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInformation {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInformation>,
}

/// Information about a single parameter within a signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterInformation {
    /// Byte offset range [start, end) within the signature label
    pub label_offsets: (u32, u32),
    pub documentation: Option<String>,
}

/// Selection range for smart expand/shrink selection.
///
/// Represents a range at a cursor position with an optional parent range
/// for progressively expanding selection. Used by `textDocument/selectionRange`.
///
/// Selection ranges form a linked list from innermost to outermost:
/// - field name → field with args → selection set → operation body → operation → document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionRange {
    /// The range of this selection
    pub range: Range,
    /// The parent selection range (enclosing scope)
    pub parent: Option<Box<SelectionRange>>,
}

impl SelectionRange {
    /// Create a new selection range without a parent
    #[must_use]
    pub const fn new(range: Range) -> Self {
        Self {
            range,
            parent: None,
        }
    }

    /// Create a new selection range with a parent
    #[must_use]
    pub fn with_parent(range: Range, parent: Self) -> Self {
        Self {
            range,
            parent: Some(Box::new(parent)),
        }
    }

    /// Build a selection range chain from a list of ranges (outermost to innermost)
    ///
    /// The first range is the outermost (document), the last is the innermost (current selection).
    #[must_use]
    pub fn from_ranges(ranges: &[Range]) -> Option<Self> {
        if ranges.is_empty() {
            return None;
        }

        let mut result = Self::new(ranges[0]);
        for range in ranges.iter().skip(1) {
            result = Self::with_parent(*range, result);
        }
        Some(result)
    }
}

use std::collections::HashMap;
use std::sync::Arc;

/// Semantic token type for syntax highlighting
///
/// These map to LSP semantic token types and provide rich syntax highlighting
/// based on semantic analysis (e.g., knowing if a field is deprecated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenType {
    /// GraphQL type names (User, Post, etc.)
    Type,
    /// Field names in selection sets
    Property,
    /// Variables ($id, $limit)
    Variable,
    /// Fragment names
    Function,
    /// Enum values (ACTIVE, PENDING)
    EnumMember,
    /// Keywords (query, mutation, fragment, on)
    Keyword,
    /// String literals
    String,
    /// Number literals
    Number,
}

impl SemanticTokenType {
    /// Index into the legend (must match order in LSP capability registration)
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Type => 0,
            Self::Property => 1,
            Self::Variable => 2,
            Self::Function => 3,
            Self::EnumMember => 4,
            Self::Keyword => 5,
            Self::String => 6,
            Self::Number => 7,
        }
    }
}

/// Semantic token modifier for additional styling
///
/// These are combined as a bitmask and provide additional semantic information
/// like deprecation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticTokenModifiers(u32);

impl SemanticTokenModifiers {
    /// No modifiers
    pub const NONE: Self = Self(0);
    /// Element is deprecated (triggers strikethrough in editors)
    pub const DEPRECATED: Self = Self(1 << 0);
    /// Element is a definition site
    pub const DEFINITION: Self = Self(1 << 1);

    /// Create from raw bitmask
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Get raw bitmask value
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Combine modifiers
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Check if a modifier is set
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// A semantic token for syntax highlighting
///
/// Tokens are emitted in document order and converted to delta encoding
/// by the LSP layer before being sent to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    /// Start position of the token
    pub start: Position,
    /// Length of the token in UTF-16 code units
    pub length: u32,
    /// Token type
    pub token_type: SemanticTokenType,
    /// Token modifiers (bitmask)
    pub modifiers: SemanticTokenModifiers,
}

impl SemanticToken {
    #[must_use]
    pub const fn new(
        start: Position,
        length: u32,
        token_type: SemanticTokenType,
        modifiers: SemanticTokenModifiers,
    ) -> Self {
        Self {
            start,
            length,
            token_type,
            modifiers,
        }
    }
}

/// Field usage information for a single field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldUsageInfo {
    /// Number of operations that use this field
    pub usage_count: usize,
    /// Names of operations that use this field
    pub operations: Vec<String>,
}

/// Coverage information for a single type
#[derive(Debug, Clone, PartialEq)]
pub struct TypeCoverageInfo {
    /// Name of the type
    pub type_name: String,
    /// Total number of fields on this type
    pub total_fields: usize,
    /// Number of fields that are used in operations
    pub used_fields: usize,
}

impl TypeCoverageInfo {
    /// Calculate coverage as a percentage (0.0 to 100.0)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn coverage_percentage(&self) -> f64 {
        if self.total_fields == 0 {
            100.0
        } else {
            (self.used_fields as f64 / self.total_fields as f64) * 100.0
        }
    }
}

/// Field usage coverage report for an entire project
#[derive(Debug, Clone, Default)]
pub struct FieldCoverageReport {
    /// Total number of fields in the schema
    pub total_fields: usize,
    /// Number of fields used in at least one operation
    pub used_fields: usize,
    /// Coverage by type
    pub types: Vec<TypeCoverageInfo>,
    /// Detailed field usages (`type_name`, `field_name`) -> usage info
    pub field_usages: HashMap<(String, String), FieldUsageInfo>,
}

impl FieldCoverageReport {
    /// Calculate overall coverage as a percentage (0.0 to 100.0)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn coverage_percentage(&self) -> f64 {
        if self.total_fields == 0 {
            100.0
        } else {
            (self.used_fields as f64 / self.total_fields as f64) * 100.0
        }
    }

    /// Get all unused fields as (`type_name`, `field_name`) tuples
    #[must_use]
    pub fn unused_fields(&self) -> Vec<(String, String)> {
        self.field_usages
            .iter()
            .filter(|(_, info)| info.usage_count == 0)
            .map(|((type_name, field_name), _)| (type_name.clone(), field_name.clone()))
            .collect()
    }
}

impl From<Arc<graphql_analysis::FieldCoverageReport>> for FieldCoverageReport {
    fn from(report: Arc<graphql_analysis::FieldCoverageReport>) -> Self {
        let types: Vec<TypeCoverageInfo> = report
            .type_coverage
            .iter()
            .map(|(name, coverage)| TypeCoverageInfo {
                type_name: name.to_string(),
                total_fields: coverage.total_fields,
                used_fields: coverage.used_fields,
            })
            .collect();

        let field_usages: HashMap<(String, String), FieldUsageInfo> = report
            .field_usages
            .iter()
            .map(|((type_name, field_name), usage)| {
                (
                    (type_name.to_string(), field_name.to_string()),
                    FieldUsageInfo {
                        usage_count: usage.usage_count,
                        operations: usage.operations.iter().map(ToString::to_string).collect(),
                    },
                )
            })
            .collect();

        Self {
            total_fields: report.total_fields,
            used_fields: report.used_fields,
            types,
            field_usages,
        }
    }
}

/// Per-field complexity breakdown
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldComplexity {
    /// Field path from root (e.g., "posts.author.name")
    pub path: String,
    /// Field name
    pub name: String,
    /// Complexity score for this field
    pub complexity: u32,
    /// List multiplier (for list fields like `[Post!]!`)
    pub multiplier: u32,
    /// Depth level (0 = root level)
    pub depth: u32,
    /// Whether this is a connection pattern (edges/nodes pagination)
    pub is_connection: bool,
    /// Warning message if any (e.g., nested pagination)
    pub warning: Option<String>,
}

impl FieldComplexity {
    pub fn new(path: impl Into<String>, name: impl Into<String>, complexity: u32) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            complexity,
            multiplier: 1,
            depth: 0,
            is_connection: false,
            warning: None,
        }
    }

    #[must_use]
    pub const fn with_multiplier(mut self, multiplier: u32) -> Self {
        self.multiplier = multiplier;
        self
    }

    #[must_use]
    pub const fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    #[must_use]
    pub const fn with_connection(mut self, is_connection: bool) -> Self {
        self.is_connection = is_connection;
        self
    }

    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warning = Some(warning.into());
        self
    }
}

/// Complexity analysis result for an operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexityAnalysis {
    /// Operation name (or "<anonymous>" for unnamed operations)
    pub operation_name: String,
    /// Operation type (query, mutation, subscription)
    pub operation_type: String,
    /// Total calculated complexity score
    pub total_complexity: u32,
    /// Maximum selection depth
    pub depth: u32,
    /// Per-field complexity breakdown
    pub breakdown: Vec<FieldComplexity>,
    /// Warnings about potential issues (nested pagination, etc.)
    pub warnings: Vec<String>,
    /// File path containing this operation
    pub file: FilePath,
    /// Range of the operation in the file
    pub range: Range,
}

impl ComplexityAnalysis {
    pub fn new(
        operation_name: impl Into<String>,
        operation_type: impl Into<String>,
        file: FilePath,
        range: Range,
    ) -> Self {
        Self {
            operation_name: operation_name.into(),
            operation_type: operation_type.into(),
            total_complexity: 0,
            depth: 0,
            breakdown: Vec::new(),
            warnings: Vec::new(),
            file,
            range,
        }
    }
}

/// A lightweight summary of a schema type for listing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaTypeEntry {
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
    pub field_count: usize,
    pub implements: Vec<String>,
    pub is_extension: bool,
}

/// Full details about a specific schema type
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
    pub implements: Vec<String>,
    pub fields: Vec<TypeFieldInfo>,
    pub directives: Vec<TypeDirectiveInfo>,
    pub enum_values: Vec<TypeEnumValueInfo>,
    pub union_members: Vec<String>,
}

/// A field in a type definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFieldInfo {
    pub name: String,
    pub type_ref: String,
    pub description: Option<String>,
    pub arguments: Vec<TypeArgumentInfo>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
    pub directives: Vec<TypeDirectiveInfo>,
}

/// An argument on a field or directive
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeArgumentInfo {
    pub name: String,
    pub type_ref: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
}

/// A directive applied to a schema element
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDirectiveInfo {
    pub name: String,
    pub arguments: Vec<TypeDirectiveArgumentInfo>,
}

/// An argument passed to a directive
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDirectiveArgumentInfo {
    pub name: String,
    pub value: String,
}

/// An enum value in an enum type
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEnumValueInfo {
    pub name: String,
    pub description: Option<String>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

/// Summary of an operation for document analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSummary {
    pub name: Option<String>,
    pub operation_type: String,
    pub file: FilePath,
    pub variables: Vec<OperationVariableInfo>,
    pub fragment_dependencies: Vec<String>,
}

/// A variable defined on an operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationVariableInfo {
    pub name: String,
    pub type_ref: String,
    pub default_value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_creation() {
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_range_creation() {
        let range = Range::new(Position::new(0, 0), Position::new(1, 10));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 1);
    }

    #[test]
    fn test_file_path_creation() {
        let path = FilePath::new("file:///path/to/file.graphql");
        assert_eq!(path.as_str(), "file:///path/to/file.graphql");

        let path2: FilePath = "test.graphql".into();
        assert_eq!(path2.as_str(), "test.graphql");
    }

    #[test]
    fn test_file_path_from_path_unix_absolute() {
        let path = FilePath::from_path(std::path::Path::new("/home/user/file.graphql"));
        assert_eq!(path.as_str(), "file:///home/user/file.graphql");
    }

    #[test]
    fn test_file_path_from_path_already_file_uri() {
        let path = FilePath::from_path(std::path::Path::new("file:///home/user/file.graphql"));
        assert_eq!(path.as_str(), "file:///home/user/file.graphql");
    }

    #[test]
    fn test_file_path_from_path_other_scheme() {
        let path = FilePath::from_path(std::path::Path::new("https://example.com/schema.graphql"));
        assert_eq!(path.as_str(), "https://example.com/schema.graphql");
    }

    #[test]
    fn test_file_path_from_path_relative() {
        let path = FilePath::from_path(std::path::Path::new("src/schema.graphql"));
        assert_eq!(path.as_str(), "file:///src/schema.graphql");
    }

    #[test]
    fn test_completion_item_builder() {
        let item = CompletionItem::new("fieldName", CompletionKind::Field)
            .with_detail("String!")
            .with_documentation("A field that returns a string")
            .with_deprecated(true);

        assert_eq!(item.label, "fieldName");
        assert_eq!(item.kind, CompletionKind::Field);
        assert_eq!(item.detail, Some("String!".to_string()));
        assert!(item.deprecated);
    }

    #[test]
    fn test_hover_result_builder() {
        let hover = HoverResult::new("```graphql\ntype User\n```")
            .with_range(Range::new(Position::new(0, 5), Position::new(0, 9)));

        assert!(hover.contents.contains("type User"));
        assert!(hover.range.is_some());
    }

    #[test]
    fn test_diagnostic_builder() {
        let diag = Diagnostic::new(
            Range::new(Position::new(1, 0), Position::new(1, 10)),
            DiagnosticSeverity::Error,
            "Unknown type: User",
            "graphql",
        )
        .with_code("unknown-type");

        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert_eq!(diag.message, "Unknown type: User");
        assert_eq!(diag.code, Some("unknown-type".to_string()));
    }

    #[test]
    fn test_project_status_total_files() {
        let status = ProjectStatus::new(3, 7);
        assert_eq!(status.total_files(), 10);
        assert_eq!(status.schema_file_count, 3);
        assert_eq!(status.document_file_count, 7);
    }

    #[test]
    fn test_project_status_has_schema() {
        let with_schema = ProjectStatus::new(1, 5);
        assert!(with_schema.has_schema());

        let without_schema = ProjectStatus::new(0, 5);
        assert!(!without_schema.has_schema());
    }

    #[test]
    fn test_project_status_default() {
        let status = ProjectStatus::default();
        assert_eq!(status.schema_file_count, 0);
        assert_eq!(status.document_file_count, 0);
        assert_eq!(status.total_files(), 0);
        assert!(!status.has_schema());
    }

    #[test]
    fn test_schema_load_result_has_no_user_schema_empty() {
        let result = SchemaLoadResult::default();
        assert!(result.has_no_user_schema());
    }

    #[test]
    fn test_schema_load_result_has_no_user_schema_with_paths() {
        let result = SchemaLoadResult {
            loaded_count: 2,
            loaded_paths: vec![std::path::PathBuf::from("schema.graphql")],
            pending_introspections: vec![],
            content_errors: vec![],
            unmatched_patterns: vec![],
        };
        assert!(!result.has_no_user_schema());
    }

    #[test]
    fn test_schema_load_result_has_no_user_schema_with_introspection() {
        let result = SchemaLoadResult {
            loaded_count: 1,
            loaded_paths: vec![],
            pending_introspections: vec![PendingIntrospection {
                url: "http://localhost:4000/graphql".to_string(),
                headers: None,
                timeout: None,
                retry: None,
            }],
            content_errors: vec![],
            unmatched_patterns: vec![],
        };
        assert!(!result.has_no_user_schema());
    }
}
