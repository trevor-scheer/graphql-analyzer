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
        {
            let mut registry = self.registry.write().unwrap();
            registry.add_file(&self.db, path, content, kind);
        } // Drop lock before rebuilding ProjectFiles
        let mut registry = self.registry.write().unwrap();
        registry.rebuild_project_files(&mut self.db);
    }

    /// Remove a file from the host
    pub fn remove_file(&mut self, path: &FilePath) {
        let file_id = {
            let registry = self.registry.read().unwrap();
            registry.get_file_id(path)
        };

        if let Some(file_id) = file_id {
            {
                let mut registry = self.registry.write().unwrap();
                registry.remove_file(file_id);
            } // Drop lock before rebuilding ProjectFiles
            let mut registry = self.registry.write().unwrap();
            registry.rebuild_project_files(&mut self.db);
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
    db: RootDatabase,
    registry: Arc<RwLock<FileRegistry>>,
}

impl Analysis {
    /// Get diagnostics for a file
    ///
    /// Returns syntax errors, validation errors, and lint warnings.
    pub fn diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
        let (content, metadata) = {
            let registry = self.registry.read().unwrap();

            // Look up FileId from FilePath
            let Some(file_id) = registry.get_file_id(file) else {
                return Vec::new();
            };

            // Get FileContent and FileMetadata
            let Some(content) = registry.get_content(file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return Vec::new();
            };
            drop(registry);

            (content, metadata)
        };

        // Get diagnostics from analysis layer
        let analysis_diagnostics = graphql_analysis::file_diagnostics(&self.db, content, metadata);

        // Convert to IDE diagnostic format
        analysis_diagnostics
            .iter()
            .map(convert_diagnostic)
            .collect()
    }

    /// Get completions at a position
    ///
    /// Returns a list of completion items appropriate for the context.
    pub fn completions(&self, file: &FilePath, position: Position) -> Option<Vec<CompletionItem>> {
        let (content, metadata) = {
            let registry = self.registry.read().unwrap();

            // Look up FileId from FilePath
            let file_id = registry.get_file_id(file)?;

            // Get FileContent and FileMetadata
            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        // Parse the file
        let parse = graphql_syntax::parse(&self.db, content, metadata);

        // Get line index for position conversion
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Convert position to byte offset
        let _offset = position_to_offset(&line_index, position)?;

        // For now, return empty completions if there are syntax errors
        if !parse.errors.is_empty() {
            return Some(Vec::new());
        }

        // TODO: Implement full completion logic with context detection
        // For now, return empty list (but Some to indicate file exists)
        Some(Vec::new())
    }

    /// Get hover information at a position
    ///
    /// Returns documentation, type information, etc.
    pub fn hover(&self, file: &FilePath, position: Position) -> Option<HoverResult> {
        let (content, metadata) = {
            let registry = self.registry.read().unwrap();

            // Look up FileId from FilePath
            let file_id = registry.get_file_id(file)?;

            // Get FileContent and FileMetadata
            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        // Parse the file
        let parse = graphql_syntax::parse(&self.db, content, metadata);

        // Get line index for position conversion
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, position)?;

        // For now, return a simple hover based on the parse tree
        // TODO: Implement full hover logic with symbol identification
        if !parse.errors.is_empty() {
            return Some(HoverResult::new(format!(
                "**Syntax Errors**\n\n{}",
                parse.errors.join("\n")
            )));
        }

        // Basic hover showing file type
        Some(HoverResult::new(format!(
            "GraphQL Document\n\nPosition: line {}, character {}\nOffset: {}",
            position.line, position.character, offset
        )))
    }

    /// Get goto definition locations for the symbol at a position
    ///
    /// Returns the definition location(s) for types, fields, fragments, etc.
    pub fn goto_definition(&self, file: &FilePath, position: Position) -> Option<Vec<Location>> {
        let (content, metadata) = {
            let registry = self.registry.read().unwrap();

            // Look up FileId from FilePath
            let file_id = registry.get_file_id(file)?;

            // Get FileContent and FileMetadata
            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        // Parse the file
        let _parse = graphql_syntax::parse(&self.db, content, metadata);

        // Get line index for position conversion
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Convert position to byte offset
        let _offset = position_to_offset(&line_index, position)?;

        // TODO: Implement full goto definition logic
        // Need to:
        // 1. Find the token/symbol at the offset
        // 2. Identify what kind of symbol it is
        // 3. Look up its definition in the HIR
        // 4. Convert to Location

        // For now, return empty list (but Some to indicate file exists)
        Some(Vec::new())
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

// Conversion functions from analysis types to IDE types

/// Convert IDE position to byte offset using `LineIndex`
fn position_to_offset(line_index: &graphql_syntax::LineIndex, position: Position) -> Option<usize> {
    let line_start = line_index.line_start(position.line as usize)?;
    Some(line_start + position.character as usize)
}

/// Convert analysis Position to IDE Position
const fn convert_position(pos: graphql_analysis::Position) -> Position {
    Position {
        line: pos.line,
        character: pos.character,
    }
}

/// Convert analysis `DiagnosticRange` to IDE Range
const fn convert_range(range: graphql_analysis::DiagnosticRange) -> Range {
    Range {
        start: convert_position(range.start),
        end: convert_position(range.end),
    }
}

/// Convert analysis Severity to IDE `DiagnosticSeverity`
const fn convert_severity(severity: graphql_analysis::Severity) -> DiagnosticSeverity {
    match severity {
        graphql_analysis::Severity::Error => DiagnosticSeverity::Error,
        graphql_analysis::Severity::Warning => DiagnosticSeverity::Warning,
        graphql_analysis::Severity::Info => DiagnosticSeverity::Information,
    }
}

/// Convert analysis Diagnostic to IDE Diagnostic
fn convert_diagnostic(diag: &graphql_analysis::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: convert_range(diag.range),
        severity: convert_severity(diag.severity),
        message: diag.message.to_string(),
        code: diag.code.as_ref().map(ToString::to_string),
        source: diag.source.to_string(),
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

    #[test]
    fn test_diagnostics_for_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a valid schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema);

        // Get diagnostics
        let snapshot = host.snapshot();
        let diagnostics = snapshot.diagnostics(&path);

        // Valid file should have no diagnostics (or only non-error diagnostics)
        // Note: There might be some diagnostics depending on validation rules
        assert!(diagnostics
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    #[test]
    fn test_diagnostics_for_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get diagnostics for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let diagnostics = snapshot.diagnostics(&path);

        // Should return empty vector for nonexistent file
        assert!(diagnostics.is_empty());
    }

    #[test]
    #[ignore = "TODO: Fix salsa update hang when modifying files"]
    fn test_diagnostics_after_file_update() {
        let mut host = AnalysisHost::new();

        // Add a file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema);

        // Get initial diagnostics
        let snapshot1 = host.snapshot();
        let diagnostics1 = snapshot1.diagnostics(&path);

        // Update the file
        host.add_file(&path, "type Query { world: Int }", FileKind::Schema);

        // Get new diagnostics
        let snapshot2 = host.snapshot();
        let diagnostics2 = snapshot2.diagnostics(&path);

        // Both should be valid (no errors)
        assert!(diagnostics1
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
        assert!(diagnostics2
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    #[test]
    fn test_conversion_position() {
        let analysis_pos = graphql_analysis::Position::new(10, 20);
        let ide_pos = convert_position(analysis_pos);

        assert_eq!(ide_pos.line, 10);
        assert_eq!(ide_pos.character, 20);
    }

    #[test]
    fn test_conversion_range() {
        let analysis_range = graphql_analysis::DiagnosticRange::new(
            graphql_analysis::Position::new(1, 5),
            graphql_analysis::Position::new(1, 10),
        );
        let ide_range = convert_range(analysis_range);

        assert_eq!(ide_range.start.line, 1);
        assert_eq!(ide_range.start.character, 5);
        assert_eq!(ide_range.end.line, 1);
        assert_eq!(ide_range.end.character, 10);
    }

    #[test]
    fn test_conversion_severity() {
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Error),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Warning),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Info),
            DiagnosticSeverity::Information
        );
    }

    #[test]
    fn test_conversion_diagnostic() {
        let analysis_diag = graphql_analysis::Diagnostic::with_source_and_code(
            graphql_analysis::Severity::Warning,
            "Test warning message",
            graphql_analysis::DiagnosticRange::new(
                graphql_analysis::Position::new(2, 0),
                graphql_analysis::Position::new(2, 10),
            ),
            "test-source",
            "TEST001",
        );

        let ide_diag = convert_diagnostic(&analysis_diag);

        assert_eq!(ide_diag.severity, DiagnosticSeverity::Warning);
        assert_eq!(ide_diag.message, "Test warning message");
        assert_eq!(ide_diag.source, "test-source");
        assert_eq!(ide_diag.code, Some("TEST001".to_string()));
        assert_eq!(ide_diag.range.start.line, 2);
        assert_eq!(ide_diag.range.start.character, 0);
        assert_eq!(ide_diag.range.end.line, 2);
        assert_eq!(ide_diag.range.end.character, 10);
    }

    #[test]
    fn test_hover_on_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema);

        // Get hover at a position
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&path, Position::new(0, 5));

        // Should return hover information
        assert!(hover.is_some());
        let hover = hover.unwrap();
        assert!(!hover.contents.is_empty());
    }

    #[test]
    fn test_hover_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get hover for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let hover = snapshot.hover(&path, Position::new(0, 0));

        // Should return None for nonexistent file
        assert!(hover.is_none());
    }

    #[test]
    fn test_hover_shows_syntax_errors() {
        let mut host = AnalysisHost::new();

        // Add a file with syntax errors
        let path = FilePath::new("file:///invalid.graphql");
        host.add_file(&path, "type Query {", FileKind::Schema);

        // Get hover
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&path, Position::new(0, 5));

        // Should return hover with error information
        assert!(hover.is_some());
        let hover = hover.unwrap();
        assert!(hover.contents.contains("Syntax Errors"));
    }

    #[test]
    fn test_position_to_offset_helper() {
        let text = "line 1\nline 2\nline 3";
        let line_index = graphql_syntax::LineIndex::new(text);

        // First line
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 0)),
            Some(0)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 5)),
            Some(5)
        );

        // Second line
        assert_eq!(
            position_to_offset(&line_index, Position::new(1, 0)),
            Some(7)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(1, 3)),
            Some(10)
        );

        // Third line
        assert_eq!(
            position_to_offset(&line_index, Position::new(2, 0)),
            Some(14)
        );
    }

    #[test]
    fn test_completions_on_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema);

        // Get completions at a position
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&path, Position::new(0, 10));

        // Should return Some (file exists) even if empty
        assert!(completions.is_some());
    }

    #[test]
    fn test_completions_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get completions for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let completions = snapshot.completions(&path, Position::new(0, 0));

        // Should return None for nonexistent file
        assert!(completions.is_none());
    }

    #[test]
    fn test_completions_with_syntax_errors() {
        let mut host = AnalysisHost::new();

        // Add a file with syntax errors
        let path = FilePath::new("file:///invalid.graphql");
        host.add_file(&path, "type Query {", FileKind::Schema);

        // Get completions
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&path, Position::new(0, 10));

        // Should return empty list for files with syntax errors
        assert!(completions.is_some());
        assert_eq!(completions.unwrap().len(), 0);
    }

    #[test]
    fn test_goto_definition_on_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema);

        // Get goto definition at a position
        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&path, Position::new(0, 10));

        // Should return Some (file exists) even if empty
        assert!(locations.is_some());
    }

    #[test]
    fn test_goto_definition_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get goto definition for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let locations = snapshot.goto_definition(&path, Position::new(0, 0));

        // Should return None for nonexistent file
        assert!(locations.is_none());
    }
}
