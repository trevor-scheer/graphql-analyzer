//! # graphql-ide
//!
//! This crate provides editor-facing IDE features for GraphQL language support.
//! It serves as the API boundary between the analysis layer and the LSP layer.
//!
//! ## Core Principle: POD Types with Public Fields
//!
//! Following rust-analyzer's design:
//! - All types are Plain Old Data (POD) structs
//! - All fields are public
//! - Types use editor coordinates (file paths, line/column positions)
//! - No GraphQL domain knowledge leaks to LSP layer
//!
//! ## Architecture
//!
//! ```text
//! LSP Layer (tower-lsp)
//!     ↓
//! graphql-ide (this crate) ← POD types, editor API
//!     ↓
//! graphql-analysis ← Query-based validation and linting
//!     ↓
//! graphql-hir ← Semantic queries
//!     ↓
//! graphql-syntax ← Parsing
//!     ↓
//! graphql-db ← Salsa database
//! ```
//!
//! ## Main Types
//!
//! - [`AnalysisHost`] - The main entry point, owns the database
//! - [`Analysis`] - Immutable snapshot for querying IDE features
//! - POD types: [`Position`], [`Range`], [`Location`], [`FilePath`]
//! - Feature types: [`CompletionItem`], [`HoverResult`], [`Diagnostic`]

use graphql_db::RootDatabase;
use std::sync::{Arc, RwLock};

mod file_registry;
pub use file_registry::FileRegistry;

// Re-export database types that IDE layer needs
pub use graphql_db::{Change, FileKind};

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

/// Completion item
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
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

/// The main analysis host
///
/// This is the entry point for all IDE features. It owns the database and
/// provides methods to apply changes and create snapshots for analysis.
pub struct AnalysisHost {
    db: RootDatabase,
    /// File registry for mapping paths to file IDs
    /// Wrapped in Arc<RwLock> so snapshots can share it
    registry: Arc<RwLock<FileRegistry>>,
}

impl AnalysisHost {
    /// Create a new analysis host with a default database
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: RootDatabase::default(),
            registry: Arc::new(RwLock::new(FileRegistry::new())),
        }
    }

    /// Add or update a file in the host
    ///
    /// This is a convenience method for adding files to the registry and database.
    pub fn add_file(&mut self, path: &FilePath, content: &str, kind: FileKind) {
        let mut registry = self.registry.write().unwrap();
        registry.add_file(&self.db, path, content, kind);
    }

    /// Remove a file from the host
    pub fn remove_file(&mut self, path: &FilePath) {
        let mut registry = self.registry.write().unwrap();
        if let Some(file_id) = registry.get_file_id(path) {
            registry.remove_file(file_id);
        }
    }

    /// Apply a change to the database
    ///
    /// Changes include adding/updating/removing files, updating configuration, etc.
    pub fn apply_change(&mut self, change: Change) {
        self.db.apply_change(change);
    }

    /// Get an immutable snapshot for analysis
    ///
    /// This snapshot can be used from multiple threads and provides all IDE features.
    /// It's cheap to create and clone (`RootDatabase` implements Clone via salsa).
    pub fn snapshot(&self) -> Analysis {
        Analysis {
            db: self.db.clone(),
            registry: Arc::clone(&self.registry),
        }
    }

    /// Get mutable access to the database (for testing)
    #[doc(hidden)]
    pub const fn db_mut(&mut self) -> &mut RootDatabase {
        &mut self.db
    }
}

impl Default for AnalysisHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable snapshot of the analysis state
///
/// Can be cheaply cloned and used from multiple threads.
/// All IDE feature queries go through this.
#[derive(Clone)]
pub struct Analysis {
    #[allow(dead_code)]
    db: RootDatabase,
    #[allow(dead_code)]
    registry: Arc<RwLock<FileRegistry>>,
}

impl Analysis {
    /// Get diagnostics for a file
    ///
    /// Returns syntax errors, validation errors, and lint warnings.
    pub const fn diagnostics(&self, _file: &FilePath) -> Vec<Diagnostic> {
        // TODO: Implement diagnostic collection
        // 1. Look up FileId from FilePath
        // 2. Call graphql_analysis::file_diagnostics
        // 3. Convert to IDE Diagnostic format
        Vec::new()
    }

    /// Get completions at a position
    ///
    /// Returns a list of completion items appropriate for the context.
    pub const fn completions(
        &self,
        _file: &FilePath,
        _position: Position,
    ) -> Option<Vec<CompletionItem>> {
        // TODO: Implement completion logic
        // 1. Parse the file
        // 2. Find token at position
        // 3. Determine completion context
        // 4. Query HIR for available items
        // 5. Convert to CompletionItems
        None
    }

    /// Get hover information at a position
    ///
    /// Returns documentation, type information, etc.
    pub const fn hover(&self, _file: &FilePath, _position: Position) -> Option<HoverResult> {
        // TODO: Implement hover logic
        // 1. Parse the file
        // 2. Find token at position
        // 3. Identify symbol
        // 4. Query HIR for symbol data
        // 5. Format as markdown
        None
    }

    /// Get goto definition locations for the symbol at a position
    ///
    /// Returns the definition location(s) for types, fields, fragments, etc.
    pub const fn goto_definition(
        &self,
        _file: &FilePath,
        _position: Position,
    ) -> Option<Vec<Location>> {
        // TODO: Implement goto definition
        // 1. Parse the file
        // 2. Find token at position
        // 3. Identify symbol type
        // 4. Look up definition in HIR
        // 5. Convert to Location
        None
    }

    /// Find all references to the symbol at a position
    ///
    /// Returns locations of all usages of types, fields, fragments, etc.
    pub const fn find_references(
        &self,
        _file: &FilePath,
        _position: Position,
        _include_declaration: bool,
    ) -> Option<Vec<Location>> {
        // TODO: Implement find references
        // 1. Parse the file
        // 2. Find token at position
        // 3. Identify symbol
        // 4. Search for all usages in HIR
        // 5. Convert to Locations
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_host_creation() {
        let host = AnalysisHost::new();
        let _snapshot = host.snapshot();
    }

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
