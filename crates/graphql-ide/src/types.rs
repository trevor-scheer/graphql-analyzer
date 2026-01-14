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

/// File path (can be URI or file path)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub String);

impl FilePath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
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
        }
    }

    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
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
}
