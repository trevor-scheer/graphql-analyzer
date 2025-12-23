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
use graphql_hir::GraphQLHirDatabase;
use std::sync::{Arc, RwLock};

mod file_registry;
pub use file_registry::FileRegistry;

mod symbol;
use symbol::{
    find_fragment_definition_range, find_fragment_spreads, find_symbol_at_offset,
    find_type_definition_range, find_type_references_in_tree, Symbol,
};

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
        let project_files = {
            let registry = self.registry.read().unwrap();
            registry.project_files()
        };

        Analysis {
            db: self.db.clone(),
            registry: Arc::clone(&self.registry),
            project_files,
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
    /// Cached `ProjectFiles` for HIR queries
    /// This is fetched from the registry when the snapshot is created
    project_files: Option<graphql_db::ProjectFiles>,
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

        // Return empty if there are syntax errors
        if !parse.errors.is_empty() {
            return Some(Vec::new());
        }

        // Get line index for position conversion
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, position)?;

        // Find what symbol we're completing (or near)
        let symbol = find_symbol_at_offset(&parse.tree, offset);

        // Determine completion context and provide appropriate completions
        match symbol {
            Some(Symbol::FragmentSpread { .. }) | None => {
                // Complete fragment names
                // If we have project files, use the new method; otherwise return empty
                let Some(project_files) = self.project_files else {
                    return Some(Vec::new());
                };
                let fragments = graphql_hir::all_fragments_with_project(&self.db, project_files);

                let items: Vec<CompletionItem> = fragments
                    .keys()
                    .map(|name| CompletionItem::new(name.to_string(), CompletionKind::Fragment))
                    .collect();

                Some(items)
            }
            Some(Symbol::FieldName { .. }) => {
                // For field completions, we'd need to determine the parent type
                // TODO: Implement proper type tracking through selection sets
                // For now, return Query type fields as a basic implementation
                let Some(project_files) = self.project_files else {
                    return Some(Vec::new());
                };
                let types = graphql_hir::schema_types_with_project(&self.db, project_files);

                types.get("Query").map_or_else(
                    || Some(Vec::new()),
                    |query_type| {
                        let items: Vec<CompletionItem> = query_type
                            .fields
                            .iter()
                            .map(|field| {
                                CompletionItem::new(field.name.to_string(), CompletionKind::Field)
                                    .with_detail(format_type_ref(&field.type_ref))
                            })
                            .collect();

                        Some(items)
                    },
                )
            }
            _ => Some(Vec::new()),
        }
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
        let parse = graphql_syntax::parse(&self.db, content, metadata);

        // Get line index for position conversion
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, position)?;

        // Find the symbol at the offset
        let symbol = find_symbol_at_offset(&parse.tree, offset)?;

        // Get project files for HIR queries
        let project_files = self.project_files?;

        // Look up the definition based on symbol type
        match symbol {
            Symbol::FragmentSpread { name } => {
                // Query HIR for all fragments
                let fragments = graphql_hir::all_fragments_with_project(&self.db, project_files);

                // Find the fragment by name
                let fragment = fragments.get(name.as_str())?;

                // Get the file content, metadata, and path for this fragment
                let registry = self.registry.read().unwrap();
                let file_path = registry.get_path(fragment.file_id)?;
                let def_content = registry.get_content(fragment.file_id)?;
                let def_metadata = registry.get_metadata(fragment.file_id)?;
                drop(registry);

                // Parse the definition file to find exact position
                let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);

                // Find the fragment definition range in the parsed tree
                if let Some((start_offset, end_offset)) =
                    find_fragment_definition_range(&def_parse.tree, &name)
                {
                    // Convert byte offsets to line/column positions
                    let def_line_index = graphql_syntax::line_index(&self.db, def_content);
                    let range = offset_range_to_range(&def_line_index, start_offset, end_offset);

                    Some(vec![Location::new(file_path, range)])
                } else {
                    // Fallback to placeholder if we can't find exact position
                    Some(vec![Location::new(
                        file_path,
                        Range::new(Position::new(0, 0), Position::new(0, 0)),
                    )])
                }
            }
            Symbol::TypeName { name } => {
                // Query HIR for all types
                let types = graphql_hir::schema_types_with_project(&self.db, project_files);

                // Find the type by name
                let type_def = types.get(name.as_str())?;

                // Get the file content, metadata, and path for this type
                let registry = self.registry.read().unwrap();
                let file_path = registry.get_path(type_def.file_id)?;
                let def_content = registry.get_content(type_def.file_id)?;
                let def_metadata = registry.get_metadata(type_def.file_id)?;
                drop(registry);

                // Parse the definition file to find exact position
                let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);

                // Find the type definition range in the parsed tree
                if let Some((start_offset, end_offset)) =
                    find_type_definition_range(&def_parse.tree, &name)
                {
                    // Convert byte offsets to line/column positions
                    let def_line_index = graphql_syntax::line_index(&self.db, def_content);
                    let range = offset_range_to_range(&def_line_index, start_offset, end_offset);

                    Some(vec![Location::new(file_path, range)])
                } else {
                    // Fallback to placeholder if we can't find exact position
                    Some(vec![Location::new(
                        file_path,
                        Range::new(Position::new(0, 0), Position::new(0, 0)),
                    )])
                }
            }
            _ => None,
        }
    }

    /// Find all references to the symbol at a position
    ///
    /// Returns locations of all usages of types, fields, fragments, etc.
    pub fn find_references(
        &self,
        file: &FilePath,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
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

        // Find the symbol at the offset
        let symbol = find_symbol_at_offset(&parse.tree, offset)?;

        // Find all references based on symbol type
        match symbol {
            Symbol::FragmentSpread { name } => {
                Some(self.find_fragment_references(&name, include_declaration))
            }
            Symbol::TypeName { name } => {
                Some(self.find_type_references(&name, include_declaration))
            }
            _ => None,
        }
    }

    /// Find all references to a fragment
    fn find_fragment_references(
        &self,
        fragment_name: &str,
        include_declaration: bool,
    ) -> Vec<Location> {
        let mut locations = Vec::new();

        // Get project files for HIR queries
        let Some(project_files) = self.project_files else {
            return locations;
        };

        // Get all fragments to find the declaration
        let fragments = graphql_hir::all_fragments_with_project(&self.db, project_files);

        // Include the declaration if requested
        if include_declaration {
            if let Some(fragment) = fragments.get(fragment_name) {
                let registry = self.registry.read().unwrap();
                if let Some(file_path) = registry.get_path(fragment.file_id) {
                    locations.push(Location::new(
                        file_path,
                        Range::new(Position::new(0, 0), Position::new(0, 0)),
                    ));
                }
                drop(registry);
            }
        }

        // Search through all document files for fragment spreads
        let document_files = project_files.document_files(&self.db);

        for (file_id, content, metadata) in document_files.iter() {
            // Parse the document
            let parse = graphql_syntax::parse(&self.db, *content, *metadata);

            // Search for fragment spreads in the parse tree
            if let Some(spread_locations) = find_fragment_spreads(&parse.tree, fragment_name) {
                let registry = self.registry.read().unwrap();
                if let Some(file_path) = registry.get_path(*file_id) {
                    for _spread_offset in spread_locations {
                        // TODO: Convert offset to position using line index
                        // For now, use placeholder
                        locations.push(Location::new(
                            file_path.clone(),
                            Range::new(Position::new(0, 0), Position::new(0, 0)),
                        ));
                    }
                }
                drop(registry);
            }
        }

        locations
    }

    /// Find all references to a type
    fn find_type_references(&self, type_name: &str, include_declaration: bool) -> Vec<Location> {
        let mut locations = Vec::new();

        // Get project files for HIR queries
        let Some(project_files) = self.project_files else {
            return locations;
        };

        // Get all types to find the declaration
        let types = graphql_hir::schema_types_with_project(&self.db, project_files);

        // Include the declaration if requested
        if include_declaration {
            if let Some(type_def) = types.get(type_name) {
                let registry = self.registry.read().unwrap();
                if let Some(file_path) = registry.get_path(type_def.file_id) {
                    locations.push(Location::new(
                        file_path,
                        Range::new(Position::new(0, 0), Position::new(0, 0)),
                    ));
                }
                drop(registry);
            }
        }

        // Search through all schema files for type references
        let schema_files = self.db.schema_files();

        for (file_id, content, metadata) in schema_files.iter() {
            // Parse the schema file
            let parse = graphql_syntax::parse(&self.db, *content, *metadata);

            // Search for type references in the parse tree
            if let Some(type_locations) = find_type_references_in_tree(&parse.tree, type_name) {
                let registry = self.registry.read().unwrap();
                if let Some(file_path) = registry.get_path(*file_id) {
                    for _type_offset in type_locations {
                        // TODO: Convert offset to position using line index
                        // For now, use placeholder
                        locations.push(Location::new(
                            file_path.clone(),
                            Range::new(Position::new(0, 0), Position::new(0, 0)),
                        ));
                    }
                }
                drop(registry);
            }
        }

        locations
    }
}

// Conversion functions from analysis types to IDE types

/// Convert IDE position to byte offset using `LineIndex`
fn position_to_offset(line_index: &graphql_syntax::LineIndex, position: Position) -> Option<usize> {
    let line_start = line_index.line_start(position.line as usize)?;
    Some(line_start + position.character as usize)
}

/// Convert byte offset to IDE Position using `LineIndex`
#[allow(clippy::cast_possible_truncation)] // Line and column numbers won't exceed u32::MAX
fn offset_to_position(line_index: &graphql_syntax::LineIndex, offset: usize) -> Position {
    let line = line_index.line_col(offset).0;
    let line_start = line_index.line_start(line).unwrap_or(0);
    let character = offset - line_start;
    Position::new(line as u32, character as u32)
}

/// Convert byte offset range to IDE Range using `LineIndex`
fn offset_range_to_range(
    line_index: &graphql_syntax::LineIndex,
    start_offset: usize,
    end_offset: usize,
) -> Range {
    let start = offset_to_position(line_index, start_offset);
    let end = offset_to_position(line_index, end_offset);
    Range::new(start, end)
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

/// Format a type reference for display (e.g., "[String!]!")
fn format_type_ref(type_ref: &graphql_hir::TypeRef) -> String {
    let mut result = type_ref.name.to_string();

    if type_ref.is_list {
        result = format!("[{result}]");
        if type_ref.inner_non_null {
            result = format!("[{}!]", type_ref.name);
        }
    }

    if type_ref.is_non_null {
        result.push('!');
    }

    result
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

        // Get goto definition at a position (may not find anything, but shouldn't crash)
        let snapshot = host.snapshot();
        let _locations = snapshot.goto_definition(&path, Position::new(0, 10));

        // Test passes if no crash occurs
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

    #[test]
    fn test_goto_definition_fragment_spread() {
        let mut host = AnalysisHost::new();

        // Add a schema (required for HIR to work properly)
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type User { id: ID! name: String }",
            FileKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { ...UserFields }";
        host.add_file(&query_file, query_text, FileKind::ExecutableGraphQL);

        // Get goto definition for the fragment spread (position at "UserFields")
        // Position should be at the start of "UserFields" after "..."
        // "query { ..." = 11 characters, so "UserFields" starts at position 11
        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, Position::new(0, 12));

        // Should find the fragment definition
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), fragment_file.as_str());

        // Verify we got real positions (not placeholder 0,0)
        assert!(
            locations[0].range.start.line > 0 || locations[0].range.start.character > 0,
            "Expected real positions, got {:?}",
            locations[0].range
        );
    }

    #[test]
    fn test_goto_definition_type_name() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(&schema_file, "type User { id: ID }", FileKind::Schema);

        // Add a fragment that references User
        let fragment_file = FilePath::new("file:///fragment.graphql");
        let fragment_text = "fragment F on User { id }";
        host.add_file(&fragment_file, fragment_text, FileKind::ExecutableGraphQL);

        // Get goto definition for the type reference (position at "User" in fragment)
        // "fragment F on " = 14 characters, so "User" starts at position 14
        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&fragment_file, Position::new(0, 14));

        // Should find the type definition
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
    }

    #[test]
    #[ignore = "TODO: Fix symbol position matching"]
    fn test_find_references_fragment() {
        let mut host = AnalysisHost::new();

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment F on User { id }",
            FileKind::ExecutableGraphQL,
        );

        // Add queries that use the fragment
        let query1_file = FilePath::new("file:///query1.graphql");
        host.add_file(&query1_file, "query { ...F }", FileKind::ExecutableGraphQL);

        let query2_file = FilePath::new("file:///query2.graphql");
        host.add_file(&query2_file, "query { ...F }", FileKind::ExecutableGraphQL);

        // Find references to the fragment (position at "F" in fragment definition)
        // "fragment " = 9 characters, so "F" starts at position 9
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&fragment_file, Position::new(0, 9), false);

        // Should find both usages but not the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    #[ignore = "TODO: Fix symbol position matching"]
    fn test_find_references_fragment_with_declaration() {
        let mut host = AnalysisHost::new();

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment F on User { id }",
            FileKind::ExecutableGraphQL,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(&query_file, "query { ...F }", FileKind::ExecutableGraphQL);

        // Find references including declaration
        // "fragment " = 9 characters, so "F" starts at position 9
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&fragment_file, Position::new(0, 9), true);

        // Should find the usage and the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    #[ignore = "TODO: Fix symbol position matching"]
    fn test_find_references_type() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let user_file = FilePath::new("file:///user.graphql");
        host.add_file(&user_file, "type User { id: ID }", FileKind::Schema);

        // Add types that reference User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(&query_file, "type Query { user: User }", FileKind::Schema);

        let mutation_file = FilePath::new("file:///mutation.graphql");
        host.add_file(
            &mutation_file,
            "type Mutation { u: User }",
            FileKind::Schema,
        );

        // Find references to the User type
        // "type " = 5 characters, so "User" starts at position 5
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&user_file, Position::new(0, 5), false);

        // Should find all usages but not the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        // Query file has 1 reference, mutation file has 1 reference = 2 total
        assert_eq!(locations.len(), 2);
    }

    #[test]
    #[ignore = "TODO: Fix symbol position matching"]
    fn test_find_references_type_with_declaration() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let user_file = FilePath::new("file:///user.graphql");
        host.add_file(&user_file, "type User { id: ID }", FileKind::Schema);

        // Add a type that references User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(&query_file, "type Query { user: User }", FileKind::Schema);

        // Find references including declaration
        // "type " = 5 characters, so "User" starts at position 5
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&user_file, Position::new(0, 5), true);

        // Should find the usage and the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }
}
