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

#[cfg(test)]
mod analysis_host_isolation;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, RwLock};

mod file_registry;
pub use file_registry::FileRegistry;

mod symbol;
use symbol::{
    extract_all_definitions, find_field_definition_full_range, find_field_definition_range,
    find_fragment_definition_full_range, find_fragment_definition_range, find_fragment_spreads,
    find_operation_definition_ranges, find_parent_type_at_offset, find_symbol_at_offset,
    find_type_definition_full_range, find_type_definition_range, find_type_references_in_tree,
    get_parent_field_path, is_in_selection_set, Symbol,
};

// Re-export database types that IDE layer needs
pub use graphql_db::FileKind;

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

/// Convert a filesystem path to a file:// URI
///
/// This handles the common case of absolute Unix paths.
/// If the path is already a URI (starts with a scheme), it's returned as-is.
fn path_to_file_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();

    // Already a URI
    if path_str.starts_with("file://") || path_str.contains("://") {
        return path_str.to_string();
    }

    // Absolute Unix path
    if path_str.starts_with('/') {
        return format!("file://{path_str}");
    }

    // For other cases (relative paths, Windows paths), just use as-is
    // A more complete implementation would handle Windows drive letters
    path_str.to_string()
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

/// Custom database that implements config traits
#[salsa::db]
#[derive(Clone)]
struct IdeDatabase {
    storage: salsa::Storage<Self>,
    lint_config: std::cell::RefCell<Arc<graphql_linter::LintConfig>>,
    extract_config: std::cell::RefCell<Arc<graphql_extract::ExtractConfig>>,
    project_files: std::cell::RefCell<Option<graphql_db::ProjectFiles>>,
}

impl Default for IdeDatabase {
    fn default() -> Self {
        Self {
            storage: salsa::Storage::default(),
            lint_config: std::cell::RefCell::new(Arc::new(graphql_linter::LintConfig::default())),
            extract_config: std::cell::RefCell::new(Arc::new(
                graphql_extract::ExtractConfig::default(),
            )),
            project_files: std::cell::RefCell::new(None),
        }
    }
}

impl IdeDatabase {}

#[salsa::db]
impl salsa::Database for IdeDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for IdeDatabase {
    fn extract_config(&self) -> Option<Arc<graphql_extract::ExtractConfig>> {
        Some(self.extract_config.borrow().clone())
    }
}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for IdeDatabase {}

#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for IdeDatabase {
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        self.lint_config.borrow().clone()
    }
}

/// The main analysis host
///
/// This is the entry point for all IDE features. It owns the database and
/// provides methods to apply changes and create snapshots for analysis.
pub struct AnalysisHost {
    db: IdeDatabase,
    /// File registry for mapping paths to file IDs
    /// Wrapped in Arc<RwLock> so snapshots can share it
    registry: Arc<RwLock<FileRegistry>>,
}

impl AnalysisHost {
    /// Create a new analysis host with a default database
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: IdeDatabase::default(),
            registry: Arc::new(RwLock::new(FileRegistry::new())),
        }
    }

    /// Add or update a file in the host
    ///
    /// This is a convenience method for adding files to the registry and database.
    /// The `line_offset` parameter is used for TypeScript/JavaScript files where GraphQL
    /// is extracted - it indicates the line number in the original source where the GraphQL starts.
    ///
    /// Returns `true` if this is a new file, `false` if it's an update to an existing file.
    ///
    /// **IMPORTANT**: Only call `rebuild_project_files()` when this returns `true` (new file).
    /// Content-only updates do NOT require rebuilding the project index.
    pub fn add_file(
        &mut self,
        path: &FilePath,
        content: &str,
        kind: FileKind,
        line_offset: u32,
    ) -> bool {
        let mut registry = self.registry.write().unwrap();
        let (_, _, _, is_new) = registry.add_file(&mut self.db, path, content, kind, line_offset);
        is_new
    }

    /// Rebuild the `ProjectFiles` index after adding/removing files
    ///
    /// This should be called after batch adding files to avoid O(n²) performance.
    /// It's relatively expensive as it iterates through all files, so avoid calling
    /// it in a loop.
    pub fn rebuild_project_files(&mut self) {
        let mut registry = self.registry.write().unwrap();
        registry.rebuild_project_files(&mut self.db);
    }

    /// Update a file and optionally create a snapshot in a single lock acquisition
    ///
    /// This is optimized for the common case of editing an existing file. It:
    /// 1. Updates the file content (cheap operation)
    /// 2. Returns a snapshot for immediate analysis
    ///
    /// This avoids the overhead of multiple lock acquisitions per keystroke.
    /// For new files, call `add_file()` followed by `rebuild_project_files()` instead.
    ///
    /// Returns `(is_new_file, Analysis)` tuple.
    pub fn update_file_and_snapshot(
        &mut self,
        path: &FilePath,
        content: &str,
        kind: FileKind,
        line_offset: u32,
    ) -> (bool, Analysis) {
        // Single lock acquisition for both operations
        let mut registry = self.registry.write().unwrap();
        let (_, _, _, is_new) = registry.add_file(&mut self.db, path, content, kind, line_offset);

        // If this is a new file, rebuild the index before creating snapshot
        if is_new {
            registry.rebuild_project_files(&mut self.db);
        }

        let project_files = registry.project_files();
        // Release the lock before creating the snapshot (no longer needed)
        drop(registry);

        // Sync project_files to the database
        *self.db.project_files.borrow_mut() = project_files;

        let snapshot = Analysis {
            db: self.db.clone(),
            registry: Arc::clone(&self.registry),
            project_files,
        };

        (is_new, snapshot)
    }

    /// Check if a file exists in this host's registry
    #[must_use]
    pub fn contains_file(&self, path: &FilePath) -> bool {
        let registry = self.registry.read().unwrap();
        registry.get_file_id(path).is_some()
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

    /// Load schema files from a project configuration
    ///
    /// This method:
    /// - Always includes Apollo Client built-in directives
    /// - Loads schema files from local paths (single file, multiple files, glob patterns)
    /// - Logs warnings for URL schemas (introspection not yet supported)
    ///
    /// Returns the number of schema files loaded.
    pub fn load_schemas_from_config(
        &mut self,
        config: &graphql_config::ProjectConfig,
        base_dir: &std::path::Path,
    ) -> anyhow::Result<usize> {
        // Always include Apollo Client built-in directives first
        const APOLLO_CLIENT_BUILTINS: &str = include_str!("apollo_client_builtins.graphql");
        self.add_file(
            &FilePath::new("apollo_client_builtins.graphql".to_string()),
            APOLLO_CLIENT_BUILTINS,
            FileKind::Schema,
            0,
        );
        let mut count = 1;

        // Get schema patterns from config
        let patterns: Vec<String> = match &config.schema {
            graphql_config::SchemaConfig::Path(s) => vec![s.clone()],
            graphql_config::SchemaConfig::Paths(arr) => arr.clone(),
        };

        for pattern in patterns {
            // Skip URLs - these would need introspection support
            if pattern.starts_with("http://") || pattern.starts_with("https://") {
                tracing::warn!("URL schemas not yet supported: {}", pattern);
                continue;
            }

            // Treat as file glob pattern
            let full_pattern = base_dir.join(&pattern).display().to_string();

            match glob::glob(&full_pattern) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        if entry.is_file() {
                            match std::fs::read_to_string(&entry) {
                                Ok(content) => {
                                    // Convert filesystem path to file:// URI for consistent lookups
                                    let file_uri = path_to_file_uri(&entry);
                                    self.add_file(
                                        &FilePath::new(file_uri),
                                        &content,
                                        FileKind::Schema,
                                        0, // No line offset for pure GraphQL files
                                    );
                                    count += 1;
                                }
                                Err(e) => {
                                    let path_display = entry.display().to_string();
                                    tracing::error!(
                                        "Failed to read schema file {path_display}: {e}"
                                    );
                                    return Err(anyhow::anyhow!(
                                        "Failed to read schema file {path_display}: {e}"
                                    ));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to expand glob pattern {full_pattern}: {e}");
                    return Err(anyhow::anyhow!(
                        "Failed to expand glob pattern {full_pattern}: {e}"
                    ));
                }
            }
        }

        tracing::info!("Loaded {} schema file(s)", count);
        Ok(count)
    }

    /// Set the lint configuration for the project
    pub fn set_lint_config(&mut self, config: graphql_linter::LintConfig) {
        *self.db.lint_config.borrow_mut() = Arc::new(config);
    }

    /// Set the extract configuration for the project
    pub fn set_extract_config(&mut self, config: graphql_extract::ExtractConfig) {
        *self.db.extract_config.borrow_mut() = Arc::new(config);
    }

    /// Get the extract configuration for the project
    pub fn get_extract_config(&self) -> graphql_extract::ExtractConfig {
        (**self.db.extract_config.borrow()).clone()
    }

    /// Get an immutable snapshot for analysis
    ///
    /// This snapshot can be used from multiple threads and provides all IDE features.
    /// It's cheap to create and clone (`RootDatabase` implements Clone via salsa).
    pub fn snapshot(&self) -> Analysis {
        let project_files = self.registry.read().unwrap().project_files();

        if let Some(ref project_files) = project_files {
            let doc_count = project_files.document_files(&self.db).files(&self.db).len();
            let schema_count = project_files.schema_files(&self.db).files(&self.db).len();
            tracing::debug!(
                "Snapshot project_files: {} schema files, {} document files",
                schema_count,
                doc_count
            );
        } else {
            tracing::warn!("Snapshot project_files is None!");
        }

        // Sync project_files to the database so queries can access it via db.project_files()
        *self.db.project_files.borrow_mut() = project_files;

        Analysis {
            db: self.db.clone(),
            registry: Arc::clone(&self.registry),
            project_files,
        }
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
    db: IdeDatabase,
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

        // Get diagnostics from analysis layer (includes both validation and linting)
        let analysis_diagnostics =
            graphql_analysis::file_diagnostics(&self.db, content, metadata, self.project_files);

        // Convert to IDE diagnostic format
        analysis_diagnostics
            .iter()
            .map(convert_diagnostic)
            .collect()
    }

    /// Get only validation diagnostics for a file (excludes custom lint rules)
    ///
    /// Returns only GraphQL spec validation errors, not custom lint rule violations.
    /// Use this for the `validate` command to avoid duplicating lint checks.
    pub fn validation_diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
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

        // Get only validation diagnostics from analysis layer
        let analysis_diagnostics = graphql_analysis::file_validation_diagnostics(
            &self.db,
            content,
            metadata,
            self.project_files,
        );

        // Convert to IDE diagnostic format
        analysis_diagnostics
            .iter()
            .map(convert_diagnostic)
            .collect()
    }

    /// Get only lint diagnostics for a file (excludes validation errors)
    ///
    /// Returns only custom lint rule violations, not GraphQL spec validation errors.
    pub fn lint_diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
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

        // Get only lint diagnostics from lint integration
        let lint_diagnostics = graphql_analysis::lint_integration::lint_file(
            &self.db,
            content,
            metadata,
            self.project_files,
        );

        // Convert to IDE diagnostic format
        lint_diagnostics.iter().map(convert_diagnostic).collect()
    }

    /// Get project-wide lint diagnostics (e.g., unused fields, unique names)
    ///
    /// Returns a map of file paths -> diagnostics for project-wide lint rules.
    /// These are expensive rules that analyze the entire project.
    pub fn project_lint_diagnostics(&self) -> HashMap<FilePath, Vec<Diagnostic>> {
        let diagnostics_by_file_id = graphql_analysis::lint_integration::project_lint_diagnostics(
            &self.db,
            self.project_files,
        );

        let mut results = HashMap::new();
        let registry = self.registry.read().unwrap();

        for (file_id, diagnostics) in diagnostics_by_file_id.iter() {
            if let Some(file_path) = registry.get_path(*file_id) {
                let converted: Vec<Diagnostic> =
                    diagnostics.iter().map(convert_diagnostic).collect();

                if !converted.is_empty() {
                    results.insert(file_path, converted);
                }
            }
        }

        results
    }

    /// Resolve a field name in a parent type to get its return type name.
    /// Handles unwrapping List and `NonNull` types to get the base type.
    fn resolve_field_type(
        parent_type_name: &str,
        field_name: &str,
        types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
    ) -> Option<String> {
        let parent_type = types.get(parent_type_name)?;
        let field = parent_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == field_name)?;

        // Unwrap the type to get the base type name
        Some(unwrap_type_to_name(&field.type_ref))
    }

    /// Get completions at a position
    ///
    /// Returns a list of completion items appropriate for the context.
    #[allow(clippy::too_many_lines)]
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

        // Adjust position for line_offset (for extracted GraphQL from TypeScript/JavaScript)
        let line_offset = metadata.line_offset(&self.db);
        let adjusted_position = adjust_position_for_line_offset(position, line_offset)?;

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, adjusted_position)?;

        // Find what symbol we're completing (or near)
        let symbol = find_symbol_at_offset(&parse.tree, offset);

        // Determine completion context and provide appropriate completions
        match symbol {
            Some(Symbol::FragmentSpread { .. }) => {
                // Complete fragment names when on a fragment spread
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
            None => {
                // No specific symbol - check if we're in a selection set
                let Some(project_files) = self.project_files else {
                    return Some(Vec::new());
                };

                let in_selection_set = is_in_selection_set(&parse.tree, offset);

                if in_selection_set {
                    // We're in a selection set - determine the parent type
                    let types = graphql_hir::schema_types_with_project(&self.db, project_files);

                    // Find what type's fields we should complete
                    let parent_ctx = find_parent_type_at_offset(&parse.tree, offset);
                    tracing::debug!("Completions: parent_ctx = {:?}", parent_ctx);

                    let parent_ctx = parent_ctx?;

                    // If immediate_parent looks like a field name, resolve it using root_type
                    let parent_type_name =
                        if parent_ctx.immediate_parent.chars().next()?.is_lowercase() {
                            // This is a field name - resolve it using the root type
                            let resolved = Self::resolve_field_type(
                                &parent_ctx.root_type,
                                &parent_ctx.immediate_parent,
                                &types,
                            );
                            tracing::debug!(
                                "Completions: resolving field '{}' on '{}' -> {:?}",
                                parent_ctx.immediate_parent,
                                parent_ctx.root_type,
                                resolved
                            );
                            resolved?
                        } else {
                            // This is already a type name
                            tracing::debug!(
                                "Completions: immediate_parent '{}' is already a type name",
                                parent_ctx.immediate_parent
                            );
                            parent_ctx.immediate_parent
                        };

                    tracing::debug!("Completions: final parent_type_name = {}", parent_type_name);

                    types.get(parent_type_name.as_str()).map_or_else(
                        || {
                            tracing::debug!(
                                "Completions: parent type '{}' not found in schema",
                                parent_type_name
                            );
                            Some(Vec::new())
                        },
                        |parent_type| {
                            let items: Vec<CompletionItem> = parent_type
                                .fields
                                .iter()
                                .map(|field| {
                                    CompletionItem::new(
                                        field.name.to_string(),
                                        CompletionKind::Field,
                                    )
                                    .with_detail(format_type_ref(&field.type_ref))
                                })
                                .collect();

                            tracing::debug!(
                                "Completions: returning {} fields for type '{}'",
                                items.len(),
                                parent_type_name
                            );
                            Some(items)
                        },
                    )
                } else {
                    // Not in a selection set - we're at document level
                    // Don't show fragment names here; user would type keywords like "query", "mutation", "fragment"
                    // TODO: In the future, consider showing operation/fragment definition keywords
                    Some(Vec::new())
                }
            }
            Some(Symbol::FieldName { .. }) => {
                // User is on an existing field name - show fields from parent type
                let Some(project_files) = self.project_files else {
                    return Some(Vec::new());
                };
                let types = graphql_hir::schema_types_with_project(&self.db, project_files);

                // Find what type's fields we should complete
                let parent_ctx = find_parent_type_at_offset(&parse.tree, offset)?;

                // If immediate_parent looks like a field name, resolve it using root_type
                let parent_type_name = if parent_ctx.immediate_parent.chars().next()?.is_lowercase()
                {
                    Self::resolve_field_type(
                        &parent_ctx.root_type,
                        &parent_ctx.immediate_parent,
                        &types,
                    )?
                } else {
                    parent_ctx.immediate_parent
                };

                types.get(parent_type_name.as_str()).map_or_else(
                    || Some(Vec::new()),
                    |parent_type| {
                        let items: Vec<CompletionItem> = parent_type
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
    #[allow(clippy::too_many_lines)]
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

        // Adjust position for line_offset (for extracted GraphQL from TypeScript/JavaScript)
        let line_offset = metadata.line_offset(&self.db);
        let adjusted_position = adjust_position_for_line_offset(position, line_offset)?;
        tracing::debug!(
            "Hover: original position {:?}, line_offset {}, adjusted position {:?}",
            position,
            line_offset,
            adjusted_position
        );

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, adjusted_position)?;

        // Try to find the symbol at the offset even if there are parse errors
        // This allows hover to work on valid parts of a file with syntax errors elsewhere
        let symbol = find_symbol_at_offset(&parse.tree, offset);

        // If we couldn't find a symbol and there are parse errors, show the errors
        if symbol.is_none() && !parse.errors.is_empty() {
            let error_messages: Vec<&str> =
                parse.errors.iter().map(|e| e.message.as_str()).collect();
            return Some(HoverResult::new(format!(
                "**Syntax Errors**\n\n{}",
                error_messages.join("\n")
            )));
        }

        // If we couldn't find a symbol, return None
        let symbol = symbol?;

        // Get project files for schema lookups
        let project_files = self.project_files?;

        // Return hover info based on symbol type
        match symbol {
            Symbol::FieldName { name } => {
                // Get the parent type to look up the field
                let types = graphql_hir::schema_types_with_project(&self.db, project_files);
                let parent_ctx = find_parent_type_at_offset(&parse.tree, offset)?;

                // Get the full field path to properly resolve nested fields
                let field_path = get_parent_field_path(&parse.tree, offset);

                // Resolve the parent type by walking the field path
                let parent_type_name = if let Some(path) = field_path {
                    // Start from root type and resolve each field in the path
                    let mut current_type = parent_ctx.root_type;
                    for field_name in &path {
                        current_type = Self::resolve_field_type(&current_type, field_name, &types)?;
                    }
                    current_type
                } else {
                    // No field path, check if immediate_parent is a type or field
                    if let Some(first_char) = parent_ctx.immediate_parent.chars().next() {
                        if first_char.is_lowercase() {
                            Self::resolve_field_type(
                                &parent_ctx.root_type,
                                &parent_ctx.immediate_parent,
                                &types,
                            )?
                        } else {
                            parent_ctx.immediate_parent
                        }
                    } else {
                        parent_ctx.root_type
                    }
                };

                tracing::debug!(
                    "Hover: resolved parent type '{}' for field '{}'",
                    parent_type_name,
                    name
                );

                // Look up the field in the parent type
                let parent_type = types.get(parent_type_name.as_str())?;
                let field = parent_type
                    .fields
                    .iter()
                    .find(|f| f.name.as_ref() == name)?;

                let mut hover_text = format!("**Field:** `{name}`\n\n");
                let field_type = format_type_ref(&field.type_ref);
                write!(hover_text, "**Type:** `{field_type}`\n\n").ok();

                if let Some(desc) = &field.description {
                    write!(hover_text, "---\n\n{desc}\n\n").ok();
                }

                Some(HoverResult::new(hover_text))
            }
            Symbol::TypeName { name } => {
                let types = graphql_hir::schema_types_with_project(&self.db, project_files);
                let type_def = types.get(name.as_str())?;

                let mut hover_text = format!("**Type:** `{name}`\n\n");
                let kind_str = match type_def.kind {
                    graphql_hir::TypeDefKind::Object => "Object",
                    graphql_hir::TypeDefKind::Interface => "Interface",
                    graphql_hir::TypeDefKind::Union => "Union",
                    graphql_hir::TypeDefKind::Enum => "Enum",
                    graphql_hir::TypeDefKind::Scalar => "Scalar",
                    graphql_hir::TypeDefKind::InputObject => "Input Object",
                };
                write!(hover_text, "**Kind:** {kind_str}\n\n").ok();

                if let Some(desc) = &type_def.description {
                    write!(hover_text, "---\n\n{desc}\n\n").ok();
                }

                Some(HoverResult::new(hover_text))
            }
            Symbol::FragmentSpread { name } => {
                let fragments = graphql_hir::all_fragments_with_project(&self.db, project_files);
                let fragment = fragments.get(name.as_str())?;

                let hover_text = format!(
                    "**Fragment:** `{}`\n\n**On Type:** `{}`\n\n",
                    name, fragment.type_condition
                );

                Some(HoverResult::new(hover_text))
            }
            _ => {
                // For other symbols, show basic info
                Some(HoverResult::new(format!("Symbol: {symbol:?}")))
            }
        }
    }

    /// Get goto definition locations for the symbol at a position
    ///
    /// Returns the definition location(s) for types, fields, fragments, etc.
    #[allow(clippy::too_many_lines)]
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

        // Adjust position for line_offset (for extracted GraphQL from TypeScript/JavaScript)
        let line_offset = metadata.line_offset(&self.db);
        let adjusted_position = adjust_position_for_line_offset(position, line_offset)?;
        tracing::debug!(
            "Goto definition: original position {:?}, line_offset {}, adjusted position {:?}",
            position,
            line_offset,
            adjusted_position
        );

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, adjusted_position)?;

        // Find the symbol at the offset
        let symbol = find_symbol_at_offset(&parse.tree, offset)?;

        // Get project files for HIR queries
        let project_files = self.project_files?;

        // Look up the definition based on symbol type
        match symbol {
            Symbol::FragmentSpread { name } => {
                // Query HIR for all fragments
                let fragments = graphql_hir::all_fragments_with_project(&self.db, project_files);

                tracing::debug!(
                    "Looking for fragment '{}', available fragments: {:?}",
                    name,
                    fragments.keys().collect::<Vec<_>>()
                );

                // Find the fragment by name
                let fragment = fragments.get(name.as_str())?;

                // Get the file content, metadata, and path for this fragment
                let registry = self.registry.read().unwrap();

                tracing::debug!(
                    "Looking up path for fragment '{}' with FileId {:?}",
                    name,
                    fragment.file_id
                );
                let all_ids = registry.all_file_ids();
                tracing::debug!("Registry has {} files", all_ids.len());
                tracing::debug!("Registry FileIds: {:?}", all_ids);

                let Some(file_path) = registry.get_path(fragment.file_id) else {
                    tracing::error!(
                        "FileId {:?} not found in registry for fragment '{}'",
                        fragment.file_id,
                        name
                    );
                    return None;
                };
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

                    // Adjust for line offset (TypeScript/JavaScript files)
                    let def_line_offset = def_metadata.line_offset(&self.db);
                    let adjusted_range = adjust_range_for_line_offset(range, def_line_offset);

                    Some(vec![Location::new(file_path, adjusted_range)])
                } else {
                    // Fallback to placeholder if we can't find exact position
                    // Still need to account for line offset in fallback
                    let def_line_offset = def_metadata.line_offset(&self.db);
                    Some(vec![Location::new(
                        file_path,
                        Range::new(
                            Position::new(def_line_offset, 0),
                            Position::new(def_line_offset, 0),
                        ),
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

                    // Adjust for line offset (TypeScript/JavaScript files)
                    let def_line_offset = def_metadata.line_offset(&self.db);
                    let adjusted_range = adjust_range_for_line_offset(range, def_line_offset);

                    Some(vec![Location::new(file_path, adjusted_range)])
                } else {
                    // Fallback to placeholder if we can't find exact position
                    // Still need to account for line offset in fallback
                    let def_line_offset = def_metadata.line_offset(&self.db);
                    Some(vec![Location::new(
                        file_path,
                        Range::new(
                            Position::new(def_line_offset, 0),
                            Position::new(def_line_offset, 0),
                        ),
                    )])
                }
            }
            Symbol::FieldName { name: field_name } => {
                // Get parent type context
                let parent_context = find_parent_type_at_offset(&parse.tree, offset)?;
                let types = graphql_hir::schema_types_with_project(&self.db, project_files);

                // Resolve the parent type by following the field path
                let field_path = get_parent_field_path(&parse.tree, offset).unwrap_or_default();
                let parent_type_name =
                    resolve_parent_type_for_field(&parent_context.root_type, &field_path, &types);

                // Find the parent type definition
                let parent_type = types.get(parent_type_name.as_str())?;

                // Get the schema file info
                let registry = self.registry.read().unwrap();
                let file_path = registry.get_path(parent_type.file_id)?;
                let def_content = registry.get_content(parent_type.file_id)?;
                let def_metadata = registry.get_metadata(parent_type.file_id)?;
                drop(registry);

                // Parse the schema file
                let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);

                // Find the field definition range
                if let Some((start_offset, end_offset)) =
                    find_field_definition_range(&def_parse.tree, &parent_type_name, &field_name)
                {
                    let def_line_index = graphql_syntax::line_index(&self.db, def_content);
                    let range = offset_range_to_range(&def_line_index, start_offset, end_offset);

                    // Adjust for line offset (TypeScript/JavaScript files)
                    let def_line_offset = def_metadata.line_offset(&self.db);
                    let adjusted_range = adjust_range_for_line_offset(range, def_line_offset);

                    Some(vec![Location::new(file_path, adjusted_range)])
                } else {
                    None
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

        // Adjust position for line_offset (for extracted GraphQL from TypeScript/JavaScript)
        let line_offset = metadata.line_offset(&self.db);
        let adjusted_position = adjust_position_for_line_offset(position, line_offset)?;
        tracing::debug!(
            "Find references: original position {:?}, line_offset {}, adjusted position {:?}",
            position,
            line_offset,
            adjusted_position
        );

        // Convert position to byte offset
        let offset = position_to_offset(&line_index, adjusted_position)?;

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
                let file_path = registry.get_path(fragment.file_id);
                let def_content = registry.get_content(fragment.file_id);
                let def_metadata = registry.get_metadata(fragment.file_id);
                drop(registry);

                if let (Some(file_path), Some(def_content), Some(def_metadata)) =
                    (file_path, def_content, def_metadata)
                {
                    // Parse the definition file to find exact position
                    let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);

                    if let Some((start_offset, end_offset)) =
                        find_fragment_definition_range(&def_parse.tree, fragment_name)
                    {
                        // Convert byte offsets to line/column positions
                        let def_line_index = graphql_syntax::line_index(&self.db, def_content);
                        let range =
                            offset_range_to_range(&def_line_index, start_offset, end_offset);

                        // Adjust for line offset (TypeScript/JavaScript files)
                        let def_line_offset = def_metadata.line_offset(&self.db);
                        let adjusted_range = adjust_range_for_line_offset(range, def_line_offset);

                        locations.push(Location::new(file_path, adjusted_range));
                    }
                }
            }
        }

        // Search through all document files for fragment spreads
        let document_files_input = project_files.document_files(&self.db);
        let document_files = document_files_input.files(&self.db);

        for (file_id, content, metadata) in document_files.iter() {
            // Parse the document
            let parse = graphql_syntax::parse(&self.db, *content, *metadata);

            // Search for fragment spreads in the parse tree
            if let Some(spread_offsets) = find_fragment_spreads(&parse.tree, fragment_name) {
                let registry = self.registry.read().unwrap();
                let file_path = registry.get_path(*file_id);
                drop(registry);

                if let Some(file_path) = file_path {
                    // Get line index for position conversion
                    let line_index = graphql_syntax::line_index(&self.db, *content);

                    // Get line offset for TypeScript/JavaScript files
                    let line_offset = metadata.line_offset(&self.db);

                    // Convert each offset to a position range
                    for spread_offset in spread_offsets {
                        // For spreads, we want to highlight just the fragment name
                        // The offset points to the start of the name
                        // We'll create a range spanning the fragment name
                        let end_offset = spread_offset + fragment_name.len();
                        let range = offset_range_to_range(&line_index, spread_offset, end_offset);

                        // Adjust for line offset
                        let adjusted_range = adjust_range_for_line_offset(range, line_offset);

                        locations.push(Location::new(file_path.clone(), adjusted_range));
                    }
                }
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
                let file_path = registry.get_path(type_def.file_id);
                let def_content = registry.get_content(type_def.file_id);
                let def_metadata = registry.get_metadata(type_def.file_id);
                drop(registry);

                if let (Some(file_path), Some(def_content), Some(def_metadata)) =
                    (file_path, def_content, def_metadata)
                {
                    // Parse the definition file to find exact position
                    let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);

                    if let Some((start_offset, end_offset)) =
                        find_type_definition_range(&def_parse.tree, type_name)
                    {
                        // Convert byte offsets to line/column positions
                        let def_line_index = graphql_syntax::line_index(&self.db, def_content);
                        let range =
                            offset_range_to_range(&def_line_index, start_offset, end_offset);

                        // Adjust for line offset (TypeScript/JavaScript files)
                        let def_line_offset = def_metadata.line_offset(&self.db);
                        let adjusted_range = adjust_range_for_line_offset(range, def_line_offset);

                        locations.push(Location::new(file_path, adjusted_range));
                    }
                }
            }
        }

        // Search through all schema files for type references
        let schema_files_input = project_files.schema_files(&self.db);
        let schema_files = schema_files_input.files(&self.db);

        for (file_id, content, metadata) in schema_files.iter() {
            // Parse the schema file
            let parse = graphql_syntax::parse(&self.db, *content, *metadata);

            // Search for type references in the parse tree
            if let Some(type_offsets) = find_type_references_in_tree(&parse.tree, type_name) {
                let registry = self.registry.read().unwrap();
                let file_path = registry.get_path(*file_id);
                drop(registry);

                if let Some(file_path) = file_path {
                    // Get line index for position conversion
                    let line_index = graphql_syntax::line_index(&self.db, *content);

                    // Get line offset for TypeScript/JavaScript files
                    let line_offset = metadata.line_offset(&self.db);

                    // Convert each offset to a position range
                    for type_offset in type_offsets {
                        // For type references, we want to highlight just the type name
                        // The offset points to the start of the name
                        // We'll create a range spanning the type name
                        let end_offset = type_offset + type_name.len();
                        let range = offset_range_to_range(&line_index, type_offset, end_offset);

                        // Adjust for line offset
                        let adjusted_range = adjust_range_for_line_offset(range, line_offset);

                        locations.push(Location::new(file_path.clone(), adjusted_range));
                    }
                }
            }
        }

        locations
    }

    /// Get document symbols for a file (hierarchical outline)
    ///
    /// Returns types, operations, and fragments with their fields as children.
    /// This powers the "Go to Symbol in Editor" (Cmd+Shift+O) feature.
    pub fn document_symbols(&self, file: &FilePath) -> Vec<DocumentSymbol> {
        let (content, metadata, file_id) = {
            let registry = self.registry.read().unwrap();

            let Some(file_id) = registry.get_file_id(file) else {
                return Vec::new();
            };

            let Some(content) = registry.get_content(file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return Vec::new();
            };
            drop(registry);

            (content, metadata, file_id)
        };

        // Parse the file
        let parse = graphql_syntax::parse(&self.db, content, metadata);
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Get line offset for TypeScript/JavaScript files
        let line_offset = metadata.line_offset(&self.db);

        // Get HIR structure for this file (for field information)
        let structure = graphql_hir::file_structure(&self.db, file_id, content, metadata);

        let mut symbols = Vec::new();

        // Extract all definitions from the parse tree
        let definitions = extract_all_definitions(&parse.tree);

        for (name, kind, ranges) in definitions {
            let range = adjust_range_for_line_offset(
                offset_range_to_range(&line_index, ranges.def_start, ranges.def_end),
                line_offset,
            );
            let selection_range = adjust_range_for_line_offset(
                offset_range_to_range(&line_index, ranges.name_start, ranges.name_end),
                line_offset,
            );

            let symbol = match kind {
                "object" => {
                    // Find fields for this type from HIR structure
                    let children = self.get_field_children(
                        &structure,
                        &name,
                        &parse.tree,
                        &line_index,
                        line_offset,
                    );
                    DocumentSymbol::new(name, SymbolKind::Type, range, selection_range)
                        .with_children(children)
                }
                "interface" => {
                    let children = self.get_field_children(
                        &structure,
                        &name,
                        &parse.tree,
                        &line_index,
                        line_offset,
                    );
                    DocumentSymbol::new(name, SymbolKind::Interface, range, selection_range)
                        .with_children(children)
                }
                "input" => {
                    let children = self.get_field_children(
                        &structure,
                        &name,
                        &parse.tree,
                        &line_index,
                        line_offset,
                    );
                    DocumentSymbol::new(name, SymbolKind::Input, range, selection_range)
                        .with_children(children)
                }
                "union" => DocumentSymbol::new(name, SymbolKind::Union, range, selection_range),
                "enum" => {
                    // For enums, we could add enum values as children
                    DocumentSymbol::new(name, SymbolKind::Enum, range, selection_range)
                }
                "scalar" => DocumentSymbol::new(name, SymbolKind::Scalar, range, selection_range),
                "query" => DocumentSymbol::new(name, SymbolKind::Query, range, selection_range),
                "mutation" => {
                    DocumentSymbol::new(name, SymbolKind::Mutation, range, selection_range)
                }
                "subscription" => {
                    DocumentSymbol::new(name, SymbolKind::Subscription, range, selection_range)
                }
                "fragment" => {
                    // Find type condition from HIR
                    let detail = structure
                        .fragments
                        .iter()
                        .find(|f| f.name.as_ref() == name)
                        .map(|f| format!("on {}", f.type_condition));
                    let mut sym =
                        DocumentSymbol::new(name, SymbolKind::Fragment, range, selection_range);
                    if let Some(d) = detail {
                        sym = sym.with_detail(d);
                    }
                    sym
                }
                _ => continue,
            };

            symbols.push(symbol);
        }

        symbols
    }

    /// Get field children for a type definition
    #[allow(clippy::unused_self)]
    fn get_field_children(
        &self,
        structure: &graphql_hir::FileStructureData,
        type_name: &str,
        tree: &apollo_parser::SyntaxTree,
        line_index: &graphql_syntax::LineIndex,
        line_offset: u32,
    ) -> Vec<DocumentSymbol> {
        // Find the type in structure
        let Some(type_def) = structure
            .type_defs
            .iter()
            .find(|t| t.name.as_ref() == type_name)
        else {
            return Vec::new();
        };

        let mut children = Vec::new();

        for field in &type_def.fields {
            if let Some(ranges) = find_field_definition_full_range(tree, type_name, &field.name) {
                let range = adjust_range_for_line_offset(
                    offset_range_to_range(line_index, ranges.def_start, ranges.def_end),
                    line_offset,
                );
                let selection_range = adjust_range_for_line_offset(
                    offset_range_to_range(line_index, ranges.name_start, ranges.name_end),
                    line_offset,
                );

                let detail = format_type_ref(&field.type_ref);
                children.push(
                    DocumentSymbol::new(
                        field.name.to_string(),
                        SymbolKind::Field,
                        range,
                        selection_range,
                    )
                    .with_detail(detail),
                );
            }
        }

        children
    }

    /// Search for workspace symbols matching a query
    ///
    /// Returns matching types, operations, and fragments across all files.
    /// This powers the "Go to Symbol in Workspace" (Cmd+T) feature.
    pub fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        let Some(project_files) = self.project_files else {
            return Vec::new();
        };

        let query_lower = query.to_lowercase();
        let mut symbols = Vec::new();

        // Search types
        let types = graphql_hir::schema_types_with_project(&self.db, project_files);
        for (name, type_def) in types.iter() {
            if name.to_lowercase().contains(&query_lower) {
                if let Some(location) = self.get_type_location(type_def) {
                    let kind = match type_def.kind {
                        graphql_hir::TypeDefKind::Object => SymbolKind::Type,
                        graphql_hir::TypeDefKind::Interface => SymbolKind::Interface,
                        graphql_hir::TypeDefKind::Union => SymbolKind::Union,
                        graphql_hir::TypeDefKind::Enum => SymbolKind::Enum,
                        graphql_hir::TypeDefKind::Scalar => SymbolKind::Scalar,
                        graphql_hir::TypeDefKind::InputObject => SymbolKind::Input,
                    };

                    symbols.push(WorkspaceSymbol::new(name.to_string(), kind, location));
                }
            }
        }

        // Search fragments
        let fragments = graphql_hir::all_fragments_with_project(&self.db, project_files);
        for (name, fragment) in fragments.iter() {
            if name.to_lowercase().contains(&query_lower) {
                if let Some(location) = self.get_fragment_location(fragment) {
                    symbols.push(
                        WorkspaceSymbol::new(name.to_string(), SymbolKind::Fragment, location)
                            .with_container(format!("on {}", fragment.type_condition)),
                    );
                }
            }
        }

        // Search operations from document files
        let document_files_input = project_files.document_files(&self.db);
        let document_files = document_files_input.files(&self.db);
        for (file_id, content, metadata) in document_files.iter() {
            let structure = graphql_hir::file_structure(&self.db, *file_id, *content, *metadata);
            for operation in &structure.operations {
                if let Some(op_name) = &operation.name {
                    if op_name.to_lowercase().contains(&query_lower) {
                        if let Some(location) = self.get_operation_location(operation) {
                            let kind = match operation.operation_type {
                                graphql_hir::OperationType::Query => SymbolKind::Query,
                                graphql_hir::OperationType::Mutation => SymbolKind::Mutation,
                                graphql_hir::OperationType::Subscription => {
                                    SymbolKind::Subscription
                                }
                            };

                            symbols.push(WorkspaceSymbol::new(op_name.to_string(), kind, location));
                        }
                    }
                }
            }
        }

        symbols
    }

    /// Get location for a type definition
    fn get_type_location(&self, type_def: &graphql_hir::TypeDef) -> Option<Location> {
        let registry = self.registry.read().unwrap();
        let file_path = registry.get_path(type_def.file_id)?;
        let content = registry.get_content(type_def.file_id)?;
        let metadata = registry.get_metadata(type_def.file_id)?;
        drop(registry);

        let parse = graphql_syntax::parse(&self.db, content, metadata);
        let line_index = graphql_syntax::line_index(&self.db, content);
        let line_offset = metadata.line_offset(&self.db);

        let ranges = find_type_definition_full_range(&parse.tree, &type_def.name)?;
        let range = adjust_range_for_line_offset(
            offset_range_to_range(&line_index, ranges.name_start, ranges.name_end),
            line_offset,
        );

        Some(Location::new(file_path, range))
    }

    /// Get location for a fragment definition
    fn get_fragment_location(&self, fragment: &graphql_hir::FragmentStructure) -> Option<Location> {
        let registry = self.registry.read().unwrap();
        let file_path = registry.get_path(fragment.file_id)?;
        let content = registry.get_content(fragment.file_id)?;
        let metadata = registry.get_metadata(fragment.file_id)?;
        drop(registry);

        let parse = graphql_syntax::parse(&self.db, content, metadata);
        let line_index = graphql_syntax::line_index(&self.db, content);
        let line_offset = metadata.line_offset(&self.db);

        let ranges = find_fragment_definition_full_range(&parse.tree, &fragment.name)?;
        let range = adjust_range_for_line_offset(
            offset_range_to_range(&line_index, ranges.name_start, ranges.name_end),
            line_offset,
        );

        Some(Location::new(file_path, range))
    }

    /// Get location for an operation definition
    fn get_operation_location(
        &self,
        operation: &graphql_hir::OperationStructure,
    ) -> Option<Location> {
        let op_name = operation.name.as_ref()?;

        let registry = self.registry.read().unwrap();
        let file_path = registry.get_path(operation.file_id)?;
        let content = registry.get_content(operation.file_id)?;
        let metadata = registry.get_metadata(operation.file_id)?;
        drop(registry);

        let parse = graphql_syntax::parse(&self.db, content, metadata);
        let line_index = graphql_syntax::line_index(&self.db, content);
        let line_offset = metadata.line_offset(&self.db);

        let ranges = find_operation_definition_ranges(&parse.tree, op_name)?;
        let range = adjust_range_for_line_offset(
            offset_range_to_range(&line_index, ranges.name_start, ranges.name_end),
            line_offset,
        );

        Some(Location::new(file_path, range))
    }
}

// Conversion functions from analysis types to IDE types

/// Adjust a position for line offset (used for extracted GraphQL from TypeScript/JavaScript)
///
/// When GraphQL is extracted from TypeScript/JavaScript files, the line numbers in the
/// LSP request are relative to the original file, but we need positions relative to the
/// extracted GraphQL. This function subtracts the `line_offset` to get the correct position.
#[allow(clippy::cast_possible_truncation)] // Line offset won't exceed u32::MAX
const fn adjust_position_for_line_offset(position: Position, line_offset: u32) -> Option<Position> {
    if line_offset == 0 {
        return Some(position);
    }

    // Subtract line_offset from the incoming position
    // If the position is before the GraphQL block, return None
    if position.line < line_offset {
        return None;
    }

    Some(Position::new(
        position.line - line_offset,
        position.character,
    ))
}

/// Add line offset to a range (used when returning positions from extracted GraphQL)
///
/// When returning positions for document symbols in TypeScript/JavaScript files,
/// we need to add the `line_offset` to convert from GraphQL-relative positions
/// back to original file positions.
const fn adjust_range_for_line_offset(range: Range, line_offset: u32) -> Range {
    if line_offset == 0 {
        return range;
    }

    Range::new(
        Position::new(range.start.line + line_offset, range.start.character),
        Position::new(range.end.line + line_offset, range.end.character),
    )
}

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

/// Resolve the parent type for a field by walking the field path
fn resolve_parent_type_for_field(
    root_type: &str,
    field_path: &[String],
    types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> String {
    let mut current_type = root_type.to_string();

    for field_name in field_path {
        if let Some(type_def) = types.get(current_type.as_str()) {
            if let Some(field) = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == field_name)
            {
                current_type = field.type_ref.name.to_string();
            } else {
                break;
            }
        } else {
            break;
        }
    }

    current_type
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
/// Unwrap a `TypeRef` to get just the base type name (without List or `NonNull` wrappers)
fn unwrap_type_to_name(type_ref: &graphql_hir::TypeRef) -> String {
    type_ref.name.to_string()
}

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
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema, 0);
        host.rebuild_project_files();

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
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema, 0);

        // Get initial diagnostics
        let snapshot1 = host.snapshot();
        let diagnostics1 = snapshot1.diagnostics(&path);

        // Update the file
        host.add_file(&path, "type Query { world: Int }", FileKind::Schema, 0);

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
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema, 0);
        host.rebuild_project_files();

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
    fn test_hover_with_syntax_errors_shows_valid_symbols() {
        let mut host = AnalysisHost::new();

        // Add a file with syntax errors (missing closing brace)
        let path = FilePath::new("file:///invalid.graphql");
        host.add_file(&path, "type Query {", FileKind::Schema, 0);
        host.rebuild_project_files();

        // Get hover on the Query type name (position 5 is in "Query")
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&path, Position::new(0, 5));

        // Should return hover info for the Query type even with syntax errors
        // This tests that hover works on valid parts of a file with syntax errors
        assert!(hover.is_some());
        let hover = hover.unwrap();
        assert!(hover.contents.contains("Query"));
        assert!(hover.contents.contains("Type"));
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
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema, 0);

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
        host.add_file(&path, "type Query {", FileKind::Schema, 0);

        host.rebuild_project_files();

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
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema, 0);

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
            0,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { ...UserFields }";
        host.add_file(&query_file, query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

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
        host.add_file(&schema_file, "type User { id: ID }", FileKind::Schema, 0);

        // Add a fragment that references User
        let fragment_file = FilePath::new("file:///fragment.graphql");
        let fragment_text = "fragment F on User { id }";
        host.add_file(
            &fragment_file,
            fragment_text,
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

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
    fn test_goto_definition_field_on_root_type() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // "query { user }" - "user" starts at position 8
        host.add_file(
            &query_file,
            "query { user }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, Position::new(0, 9));

        assert!(locations.is_some(), "Should find field definition");
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "user" field in Query type (line 0)
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_nested_field() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { name: String }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // "query { user { name } }" - "name" starts at position 15
        host.add_file(
            &query_file,
            "query { user { name } }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, Position::new(0, 16));

        assert!(locations.is_some(), "Should find nested field definition");
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "name" field in User type (line 1)
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_schema_field_type() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        // "type Query { user: User }" - "User" return type starts at position 19
        // "type User { id: ID! }" on line 1
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! }",
            FileKind::Schema,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Click on "User" in "user: User"
        let locations = snapshot.goto_definition(&schema_file, Position::new(0, 20));

        assert!(
            locations.is_some(),
            "Should find type definition from field return type"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "User" type definition (line 1)
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_find_references_fragment() {
        let mut host = AnalysisHost::new();

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment F on User { id }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add queries that use the fragment
        let query1_file = FilePath::new("file:///query1.graphql");
        host.add_file(
            &query1_file,
            "query { ...F }",
            FileKind::ExecutableGraphQL,
            0,
        );

        let query2_file = FilePath::new("file:///query2.graphql");
        host.add_file(
            &query2_file,
            "query { ...F }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

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
    fn test_find_references_fragment_with_declaration() {
        let mut host = AnalysisHost::new();

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment F on User { id }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { ...F }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

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
    fn test_find_references_type() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let user_file = FilePath::new("file:///user.graphql");
        host.add_file(&user_file, "type User { id: ID }", FileKind::Schema, 0);

        // Add types that reference User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "type Query { user: User }",
            FileKind::Schema,
            0,
        );

        let mutation_file = FilePath::new("file:///mutation.graphql");
        host.add_file(
            &mutation_file,
            "type Mutation { u: User }",
            FileKind::Schema,
            0,
        );
        host.rebuild_project_files();

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
    fn test_find_references_type_with_declaration() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let user_file = FilePath::new("file:///user.graphql");
        host.add_file(&user_file, "type User { id: ID }", FileKind::Schema, 0);

        // Add a type that references User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "type Query { user: User }",
            FileKind::Schema,
            0,
        );
        host.rebuild_project_files();

        // Find references including declaration
        // "type " = 5 characters, so "User" starts at position 5
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&user_file, Position::new(0, 5), true);

        // Should find the usage and the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_completions_in_selection_set_should_not_show_fragments() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add a query with cursor in selection set
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { user { id } }";
        //                                 ^ cursor here at position 15 (right after { before id)
        host.add_file(&query_file, query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        // Get completions inside the selection set (simulating user about to type)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&query_file, Position::new(0, 15));

        // Should return field completions only (id, name), NOT fragment names
        assert!(completions.is_some());
        let items = completions.unwrap();

        // Check that we got field completions
        let field_names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(
            field_names.contains(&"id"),
            "Expected 'id' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"name"),
            "Expected 'name' field in completions, got: {field_names:?}"
        );

        // Check that we did NOT get fragment completions
        assert!(
            !field_names.contains(&"UserFields"),
            "Fragment names should not appear in field completions, but found 'UserFields'"
        );

        // All completions should be fields, not fragments
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Field,
                "Expected only Field completions, but found {:?} for '{}'",
                item.kind,
                item.label
            );
        }
    }

    #[test]
    fn test_completions_outside_selection_set_should_not_show_fragments() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add a query with cursor OUTSIDE any selection set (at document level)
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { user { id } }\n";
        //                                       ^ cursor at end (position 22 on line 0)
        host.add_file(&query_file, query_text, FileKind::ExecutableGraphQL, 0);

        // Get completions at document level (NOT in a selection set)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&query_file, Position::new(0, 22));

        // At document level, we shouldn't show fragment names either
        // (user would want to type "query", "mutation", "fragment", etc.)
        if let Some(items) = completions {
            let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
            assert!(
                !labels.contains(&"UserFields"),
                "Fragment names should not appear outside selection sets, but found 'UserFields'. Got: {labels:?}"
            );
        }
    }

    #[test]
    fn test_completions_after_fragment_spread_in_mutation() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Mutation { forfeitBattle(battleId: ID!, trainerId: ID!): Battle } type Battle { id: ID! status: String winner: String }",
            FileKind::Schema,
            0,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment BattleDetailed on Battle { id status }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add a mutation with cursor after fragment spread
        let mutation_file = FilePath::new("file:///mutation.graphql");
        let mutation_text = r"mutation ForfeitBattle($battleId: ID!, $trainerId: ID!) {
  forfeitBattle(battleId: $battleId, trainerId: $trainerId) {
    ...BattleDetailed

  }
}";
        host.add_file(
            &mutation_file,
            mutation_text,
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        // Get completions after the fragment spread (line 3, position 4 - after newline)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&mutation_file, Position::new(3, 4));

        // Should return field completions for Battle type
        assert!(completions.is_some(), "Expected completions to be Some");
        let items = completions.unwrap();

        let field_names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        dbg!(&field_names);

        assert!(
            field_names.contains(&"id"),
            "Expected 'id' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"status"),
            "Expected 'status' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"winner"),
            "Expected 'winner' field in completions, got: {field_names:?}"
        );
    }

    #[test]
    fn test_completions_with_multiple_mutations_in_same_file() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Mutation { forfeitBattle(battleId: ID!, trainerId: ID!): Battle startBattle(trainerId: ID!): Battle } type Battle { id: ID! status: String winner: String }",
            FileKind::Schema,
            0,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment BattleDetailed on Battle { id status }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add multiple mutations in the same file
        let mutation_file = FilePath::new("file:///mutations.graphql");
        let mutation_text = r"mutation StartBattle($trainerId: ID!) {
  startBattle(trainerId: $trainerId) {
    ...BattleDetailed
  }
}

# Forfeit a battle
mutation ForfeitBattle($battleId: ID!, $trainerId: ID!) {
  forfeitBattle(battleId: $battleId, trainerId: $trainerId) {
    ...BattleDetailed

  }
}";
        host.add_file(
            &mutation_file,
            mutation_text,
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        // Get completions in the second mutation after the fragment spread (line 10, position 4)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&mutation_file, Position::new(10, 4));

        // Should return field completions for Battle type
        assert!(
            completions.is_some(),
            "Expected completions to be Some, but got None"
        );
        let items = completions.unwrap();

        let field_names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        dbg!(&field_names);

        assert!(
            !field_names.is_empty(),
            "Expected non-empty completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"id"),
            "Expected 'id' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"status"),
            "Expected 'status' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"winner"),
            "Expected 'winner' field in completions, got: {field_names:?}"
        );
    }

    #[test]
    fn test_typescript_graphql_extraction() {
        use graphql_extract::{extract_from_source, ExtractConfig, Language};

        // Test that TypeScript files with GraphQL are correctly extracted
        // and don't produce TypeScript syntax errors like "Unexpected <EOF>" on "import"

        let typescript_source = r"import { gql } from '@apollo/client';

export const GET_POKEMON = gql`
  query GetPokemon {
    pokemon {
      id
      name
    }
  }
`;
";

        // Test extraction works
        let config = ExtractConfig::default();
        let result = extract_from_source(typescript_source, Language::TypeScript, &config);

        assert!(result.is_ok(), "Extraction should succeed");
        let blocks = result.unwrap();
        assert!(
            !blocks.is_empty(),
            "Should extract at least one GraphQL block"
        );

        // Verify extracted content
        let graphql = &blocks[0].source;
        assert!(graphql.contains("GetPokemon"), "Should contain query name");
        assert!(
            !graphql.contains("import"),
            "Should NOT contain TypeScript import statement"
        );
        assert!(
            graphql.contains("pokemon"),
            "Should contain field selections"
        );
    }

    #[test]
    fn test_document_symbols_type_with_fields() {
        let mut host = AnalysisHost::new();

        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type User {\n  id: ID!\n  name: String\n  email: String!\n}",
            FileKind::Schema,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);

        assert_eq!(symbols.len(), 1, "Should have one type symbol");
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
        assert_eq!(symbols[0].children.len(), 3, "Should have 3 field children");

        // Check field names
        let field_names: Vec<&str> = symbols[0]
            .children
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"email"));

        // Check field kinds
        for child in &symbols[0].children {
            assert_eq!(child.kind, SymbolKind::Field);
        }
    }

    #[test]
    fn test_document_symbols_operations() {
        let mut host = AnalysisHost::new();

        // Add schema first
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: String }\ntype Mutation { createUser: String }",
            FileKind::Schema,
            0,
        );

        let path = FilePath::new("file:///queries.graphql");
        host.add_file(
            &path,
            "query GetUser { user }\nmutation CreateUser { createUser }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);

        assert_eq!(symbols.len(), 2, "Should have two operation symbols");

        // Check query
        assert_eq!(symbols[0].name, "GetUser");
        assert_eq!(symbols[0].kind, SymbolKind::Query);

        // Check mutation
        assert_eq!(symbols[1].name, "CreateUser");
        assert_eq!(symbols[1].kind, SymbolKind::Mutation);
    }

    #[test]
    fn test_document_symbols_fragments() {
        let mut host = AnalysisHost::new();

        // Add schema
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type User { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        let path = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &path,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);

        assert_eq!(symbols.len(), 1, "Should have one fragment symbol");
        assert_eq!(symbols[0].name, "UserFields");
        assert_eq!(symbols[0].kind, SymbolKind::Fragment);
        assert_eq!(symbols[0].detail, Some("on User".to_string()));
    }

    #[test]
    fn test_workspace_symbols_search() {
        let mut host = AnalysisHost::new();

        // Add schema with multiple types
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { id: ID! }\ntype Post { title: String }",
            FileKind::Schema,
            0,
        );

        // Add operations
        let queries_path = FilePath::new("file:///queries.graphql");
        host.add_file(
            &queries_path,
            "query GetUser { user { id } }\nquery GetUsers { user { id } }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Search for "User"
        let symbols = snapshot.workspace_symbols("User");
        assert!(!symbols.is_empty(), "Should find symbols matching 'User'");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"User"), "Should find User type");
        assert!(names.contains(&"GetUser"), "Should find GetUser operation");
        assert!(
            names.contains(&"GetUsers"),
            "Should find GetUsers operation"
        );

        // Search for "Post"
        let symbols = snapshot.workspace_symbols("Post");
        assert_eq!(symbols.len(), 1, "Should find one symbol matching 'Post'");
        assert_eq!(symbols[0].name, "Post");
    }

    #[test]
    fn test_workspace_symbols_case_insensitive() {
        let mut host = AnalysisHost::new();

        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type UserProfile { id: ID! }", FileKind::Schema, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Search with different cases
        let lower = snapshot.workspace_symbols("user");
        let upper = snapshot.workspace_symbols("USER");
        let mixed = snapshot.workspace_symbols("uSeR");

        assert_eq!(lower.len(), 1);
        assert_eq!(upper.len(), 1);
        assert_eq!(mixed.len(), 1);

        assert_eq!(lower[0].name, "UserProfile");
        assert_eq!(upper[0].name, "UserProfile");
        assert_eq!(mixed[0].name, "UserProfile");
    }
}
