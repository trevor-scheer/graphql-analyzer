/// # graphql-ide
///
/// This crate provides editor-facing IDE features for GraphQL language support.
/// It serves as the API boundary between the analysis layer and the LSP layer.
///
/// ## Core Principle: POD Types with Public Fields
///
/// Following rust-analyzer's design:
/// - All types are Plain Old Data (POD) structs
/// - All fields are public
/// - Types use editor coordinates (file paths, line/column positions)
/// - No GraphQL domain knowledge leaks to LSP layer
///
/// ## Architecture
///
/// ```text
/// LSP Layer (tower-lsp)
///     ↓
/// graphql-ide (this crate) ← POD types, editor API
///     ↓
/// graphql-analysis ← Query-based validation and linting
///     ↓
/// graphql-hir ← Semantic queries
///     ↓
/// graphql-syntax ← Parsing
///     ↓
/// graphql-db ← Salsa database
/// ```
///
/// ## Main Types
///
/// - [`AnalysisHost`] - The main entry point, owns the database
/// - [`Analysis`] - Immutable snapshot for querying IDE features
/// - POD types: [`Position`], [`Range`], [`Location`], [`FilePath`]
/// - Feature types: [`CompletionItem`], [`HoverResult`], [`Diagnostic`]
#[cfg(test)]
mod analysis_host_isolation;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use parking_lot::RwLock;

mod file_registry;
pub use file_registry::FileRegistry;

mod symbol;
use symbol::{
    extract_all_definitions, find_field_definition_full_range, find_fragment_definition_full_range,
    find_fragment_definition_range, find_fragment_spreads, find_operation_definition_ranges,
    find_parent_type_at_offset, find_schema_field_parent_type, find_symbol_at_offset,
    find_type_definition_full_range, find_type_definition_range, find_type_references_in_tree,
    is_in_selection_set, Symbol,
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

#[cfg(test)]
/// Helper for tests: extracts cursor position from a string with a `*` marker.
///
/// # Example
/// ```ignore
/// let (source, pos) = extract_cursor("query { user*Name }");
/// assert_eq!(source, "query { userName }");
/// assert_eq!(pos, Position::new(0, 12));
/// ```
///
/// For multiline:
/// ```ignore
/// let (source, pos) = extract_cursor("query {\n  user*Name\n}");
/// assert_eq!(pos, Position::new(1, 6)); // line 1, col 6
/// ```
fn extract_cursor(input: &str) -> (String, Position) {
    let mut line = 0u32;
    let mut character = 0u32;
    let mut found = false;
    let mut result = String::with_capacity(input.len());

    for ch in input.chars() {
        if ch == '*' && !found {
            found = true;
            continue;
        }

        if !found {
            // Before cursor: track position normally
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }

        result.push(ch);
    }

    assert!(found, "No cursor marker '*' found in input");

    (result, Position::new(line, character))
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

/// Custom database that implements config traits
///
/// Uses `parking_lot::RwLock` for interior mutability to ensure thread-safety
/// when `Analysis` snapshots are accessed from multiple threads concurrently.
#[salsa::db]
#[derive(Clone)]
struct IdeDatabase {
    storage: salsa::Storage<Self>,
    lint_config: Arc<RwLock<Arc<graphql_linter::LintConfig>>>,
    extract_config: Arc<RwLock<Arc<graphql_extract::ExtractConfig>>>,
    project_files: Arc<RwLock<Option<graphql_db::ProjectFiles>>>,
}

impl Default for IdeDatabase {
    fn default() -> Self {
        Self {
            storage: salsa::Storage::default(),
            lint_config: Arc::new(RwLock::new(Arc::new(graphql_linter::LintConfig::default()))),
            extract_config: Arc::new(RwLock::new(Arc::new(
                graphql_extract::ExtractConfig::default(),
            ))),
            project_files: Arc::new(RwLock::new(None)),
        }
    }
}

#[salsa::db]
impl salsa::Database for IdeDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for IdeDatabase {
    fn extract_config(&self) -> Option<Arc<graphql_extract::ExtractConfig>> {
        Some(self.extract_config.read().clone())
    }
}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for IdeDatabase {}

#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for IdeDatabase {
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        self.lint_config.read().clone()
    }
}

/// The main analysis host
///
/// This is the entry point for all IDE features. It owns the database and
/// provides methods to apply changes and create snapshots for analysis.
///
/// # Snapshot Lifecycle (Important!)
///
/// This follows Salsa's single-writer, multi-reader model:
/// - Call [`snapshot()`](Self::snapshot) to create an immutable [`Analysis`] for queries
/// - **All snapshots must be dropped before calling any mutating method**
/// - Failing to drop snapshots before mutation will cause a hang/deadlock
///
/// ```ignore
/// // CORRECT: Scope snapshots so they're dropped before mutation
/// let result = {
///     let snapshot = host.snapshot();
///     snapshot.diagnostics(&file)
/// }; // snapshot dropped here
/// host.add_file(&file, new_content, kind, 0); // Safe: no snapshots exist
///
/// // WRONG: Holding snapshot across mutation
/// let snapshot = host.snapshot();
/// let result = snapshot.diagnostics(&file);
/// host.add_file(&file, new_content, kind, 0); // HANGS: snapshot still alive!
/// ```
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
        let mut registry = self.registry.write();
        let (_, _, _, is_new) = registry.add_file(&mut self.db, path, content, kind, line_offset);
        is_new
    }

    /// Rebuild the `ProjectFiles` index after adding/removing files
    ///
    /// This should be called after batch adding files to avoid O(n²) performance.
    /// It's relatively expensive as it iterates through all files, so avoid calling
    /// it in a loop.
    pub fn rebuild_project_files(&mut self) {
        let mut registry = self.registry.write();
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
        let mut registry = self.registry.write();
        let (_, _, _, is_new) = registry.add_file(&mut self.db, path, content, kind, line_offset);

        // If this is a new file, rebuild the index before creating snapshot
        if is_new {
            registry.rebuild_project_files(&mut self.db);
        }

        let project_files = registry.project_files();
        // Release the lock before creating the snapshot (no longer needed)
        drop(registry);

        // Sync project_files to the database
        *self.db.project_files.write() = project_files;

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
        let registry = self.registry.read();
        registry.get_file_id(path).is_some()
    }

    /// Remove a file from the host
    pub fn remove_file(&mut self, path: &FilePath) {
        let mut registry = self.registry.write();
        if let Some(file_id) = registry.get_file_id(path) {
            registry.remove_file(file_id);
            registry.rebuild_project_files(&mut self.db);
        }
    }

    /// Load schema files from a project configuration
    ///
    /// This method:
    /// - Always includes Apollo Client built-in directives
    /// - Loads schema files from local paths (single file, multiple files, glob patterns)
    /// - Supports TypeScript/JavaScript files with embedded GraphQL schemas
    /// - Logs warnings for URL schemas (introspection not yet supported)
    ///
    /// Returns the number of schema files loaded.
    #[allow(clippy::too_many_lines)]
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

        let patterns: Vec<String> = match &config.schema {
            graphql_config::SchemaConfig::Path(s) => vec![s.clone()],
            graphql_config::SchemaConfig::Paths(arr) => arr.clone(),
            graphql_config::SchemaConfig::Introspection(introspection) => {
                // Introspection schemas need to be loaded via the introspection client
                // This method only handles local files
                tracing::warn!(
                    "Introspection schema config not yet supported in IDE: {}",
                    introspection.url
                );
                vec![]
            }
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
                                    let file_uri = path_to_file_uri(&entry);
                                    let language = graphql_extract::Language::from_path(&entry);

                                    // Check if this is a TS/JS file that needs extraction
                                    if let Some(lang) = language {
                                        if lang.requires_parsing() {
                                            // Extract GraphQL from TS/JS file
                                            let extract_config = self.get_extract_config();
                                            match graphql_extract::extract_from_source(
                                                &content,
                                                lang,
                                                &extract_config,
                                            ) {
                                                Ok(blocks) => {
                                                    for (block_idx, block) in
                                                        blocks.iter().enumerate()
                                                    {
                                                        // Create a unique file URI for each block
                                                        let block_uri = if blocks.len() > 1 {
                                                            format!("{file_uri}#block{block_idx}")
                                                        } else {
                                                            file_uri.clone()
                                                        };

                                                        #[allow(clippy::cast_possible_truncation)]
                                                        let line_offset =
                                                            block.location.range.start.line as u32;
                                                        self.add_file(
                                                            &FilePath::new(block_uri),
                                                            &block.source,
                                                            FileKind::Schema,
                                                            line_offset,
                                                        );
                                                        count += 1;
                                                    }
                                                    if blocks.is_empty() {
                                                        tracing::debug!(
                                                            "No GraphQL blocks found in {}",
                                                            entry.display()
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        "Failed to extract GraphQL from {}: {}",
                                                        entry.display(),
                                                        e
                                                    );
                                                }
                                            }
                                            continue;
                                        }
                                    }

                                    // Pure GraphQL file - add directly
                                    self.add_file(
                                        &FilePath::new(file_uri),
                                        &content,
                                        FileKind::Schema,
                                        0,
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
        *self.db.lint_config.write() = Arc::new(config);
    }

    /// Set the extract configuration for the project
    pub fn set_extract_config(&mut self, config: graphql_extract::ExtractConfig) {
        *self.db.extract_config.write() = Arc::new(config);
    }

    /// Get the extract configuration for the project
    pub fn get_extract_config(&self) -> graphql_extract::ExtractConfig {
        (**self.db.extract_config.read()).clone()
    }

    /// Get an immutable snapshot for analysis
    ///
    /// This snapshot can be used from multiple threads and provides all IDE features.
    /// It's cheap to create and clone (`RootDatabase` implements Clone via salsa).
    ///
    /// # Lifecycle Warning
    ///
    /// The returned `Analysis` **must be dropped before calling any mutating method**
    /// on this `AnalysisHost`. This is required by Salsa's single-writer model.
    /// See the struct-level documentation for details and examples.
    pub fn snapshot(&self) -> Analysis {
        let project_files = self.registry.read().project_files();

        if let Some(ref project_files) = project_files {
            let doc_count = project_files
                .document_file_ids(&self.db)
                .ids(&self.db)
                .len();
            let schema_count = project_files.schema_file_ids(&self.db).ids(&self.db).len();
            tracing::debug!(
                "Snapshot project_files: {} schema files, {} document files",
                schema_count,
                doc_count
            );
        } else {
            tracing::warn!("Snapshot project_files is None!");
        }

        // Sync project_files to the database so queries can access it via db.project_files()
        *self.db.project_files.write() = project_files;

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
///
/// # Lifecycle Warning
///
/// This snapshot shares Salsa storage with its parent [`AnalysisHost`].
/// **You must drop all `Analysis` instances before calling any mutating method**
/// on the host (like `add_file`, `remove_file`, etc.). Failure to do so will
/// cause a hang/deadlock due to Salsa's single-writer, multi-reader model.
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
            let registry = self.registry.read();

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

            (content, metadata)
        };

        let analysis_diagnostics =
            graphql_analysis::file_diagnostics(&self.db, content, metadata, self.project_files);

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
            let registry = self.registry.read();

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

            (content, metadata)
        };

        let analysis_diagnostics = graphql_analysis::file_validation_diagnostics(
            &self.db,
            content,
            metadata,
            self.project_files,
        );

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
            let registry = self.registry.read();

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

            (content, metadata)
        };

        let lint_diagnostics = graphql_analysis::lint_integration::lint_file(
            &self.db,
            content,
            metadata,
            self.project_files,
        );

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
        let registry = self.registry.read();

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

    /// Get completions at a position
    ///
    /// Returns a list of completion items appropriate for the context.
    #[allow(clippy::too_many_lines)]
    pub fn completions(&self, file: &FilePath, position: Position) -> Option<Vec<CompletionItem>> {
        let (content, metadata) = {
            let registry = self.registry.read();

            let file_id = registry.get_file_id(file)?;

            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        let parse = graphql_syntax::parse(&self.db, content, metadata);

        let metadata_line_offset = metadata.line_offset(&self.db);
        let (block_context, adjusted_position) =
            find_block_for_position(&parse, position, metadata_line_offset)?;

        let offset = if let Some(block_source) = block_context.block_source {
            let block_line_index = graphql_syntax::LineIndex::new(block_source);
            position_to_offset(&block_line_index, adjusted_position)?
        } else {
            let line_index = graphql_syntax::line_index(&self.db, content);
            position_to_offset(&line_index, adjusted_position)?
        };

        // Find what symbol we're completing (or near) using the correct tree
        let symbol = find_symbol_at_offset(block_context.tree, offset);

        // Determine completion context and provide appropriate completions
        match symbol {
            Some(Symbol::FragmentSpread { .. }) => {
                // Complete fragment names when on a fragment spread
                let Some(project_files) = self.project_files else {
                    return Some(Vec::new());
                };
                let fragments = graphql_hir::all_fragments(&self.db, project_files);

                let items: Vec<CompletionItem> = fragments
                    .keys()
                    .map(|name| CompletionItem::new(name.to_string(), CompletionKind::Fragment))
                    .collect();

                Some(items)
            }
            None | Some(Symbol::FieldName { .. }) => {
                // Show fields from parent type in selection set or on field name
                let Some(project_files) = self.project_files else {
                    return Some(Vec::new());
                };
                let types = graphql_hir::schema_types(&self.db, project_files);

                let in_selection_set = is_in_selection_set(block_context.tree, offset);
                if in_selection_set {
                    // Use a stack-based type walker to resolve the parent type at the cursor
                    let parent_ctx = find_parent_type_at_offset(block_context.tree, offset)?;
                    let parent_type_name = symbol::walk_type_stack_to_offset(
                        block_context.tree,
                        &types,
                        offset,
                        &parent_ctx.root_type,
                    )?;

                    types.get(parent_type_name.as_str()).map_or_else(
                        || Some(Vec::new()),
                        |parent_type| {
                            // For union types, suggest inline fragments for each union member
                            if parent_type.kind == graphql_hir::TypeDefKind::Union {
                                let items: Vec<CompletionItem> = parent_type
                                    .union_members
                                    .iter()
                                    .map(|member| {
                                        CompletionItem::new(
                                            format!("... on {member}"),
                                            CompletionKind::Type,
                                        )
                                        .with_insert_text(format!("... on {member} {{\n  $0\n}}"))
                                        .with_insert_text_format(InsertTextFormat::Snippet)
                                    })
                                    .collect();
                                return Some(items);
                            }

                            // For object types and interfaces, suggest fields
                            let mut items: Vec<CompletionItem> = parent_type
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

                            // If interface, add inline fragment suggestions for implementing types
                            // (fields from implementing types are only accessible via inline fragments)
                            if parent_type.kind == graphql_hir::TypeDefKind::Interface {
                                for type_def in types.values() {
                                    if type_def.implements.contains(&parent_type.name) {
                                        // Add inline fragment suggestion for this implementing type
                                        let type_name = &type_def.name;
                                        let inline_fragment_label = format!("... on {type_name}");
                                        if !items
                                            .iter()
                                            .any(|i| i.label.as_str() == inline_fragment_label)
                                        {
                                            items.push(
                                                CompletionItem::new(
                                                    inline_fragment_label,
                                                    CompletionKind::Type,
                                                )
                                                .with_insert_text(format!(
                                                    "... on {type_name} {{\n  $0\n}}"
                                                ))
                                                .with_insert_text_format(InsertTextFormat::Snippet)
                                                .with_sort_text(format!("z_{type_name}")), // Sort after fields
                                            );
                                        }
                                    }
                                }
                            }
                            Some(items)
                        },
                    )
                } else {
                    // Not in a selection set - we're at document level
                    Some(Vec::new())
                }
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
            let registry = self.registry.read();

            let file_id = registry.get_file_id(file)?;

            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        let parse = graphql_syntax::parse(&self.db, content, metadata);

        let line_index = graphql_syntax::line_index(&self.db, content);

        let metadata_line_offset = metadata.line_offset(&self.db);
        let (block_context, adjusted_position) =
            find_block_for_position(&parse, position, metadata_line_offset)?;

        tracing::debug!(
            "Hover: original position {:?}, block line_offset {}, adjusted position {:?}",
            position,
            block_context.line_offset,
            adjusted_position
        );

        let offset = if let Some(block_source) = block_context.block_source {
            let block_line_index = graphql_syntax::LineIndex::new(block_source);
            position_to_offset(&block_line_index, adjusted_position)?
        } else {
            position_to_offset(&line_index, adjusted_position)?
        };

        // Try to find the symbol at the offset even if there are parse errors
        // This allows hover to work on valid parts of a file with syntax errors elsewhere
        let symbol = find_symbol_at_offset(block_context.tree, offset);

        // If we couldn't find a symbol and there are parse errors, show the errors
        if symbol.is_none() && !parse.errors.is_empty() {
            let error_messages: Vec<&str> =
                parse.errors.iter().map(|e| e.message.as_str()).collect();
            return Some(HoverResult::new(format!(
                "**Syntax Errors**\n\n{}",
                error_messages.join("\n")
            )));
        }

        let symbol = symbol?;

        let project_files = self.project_files?;

        match symbol {
            Symbol::FieldName { name } => {
                let types = graphql_hir::schema_types(&self.db, project_files);
                let parent_ctx = find_parent_type_at_offset(&parse.tree, offset)?;

                // Use walk_type_stack_to_offset to properly resolve the parent type,
                // which handles inline fragments correctly
                let parent_type_name = symbol::walk_type_stack_to_offset(
                    &parse.tree,
                    &types,
                    offset,
                    &parent_ctx.root_type,
                )?;

                tracing::debug!(
                    "Hover: resolved parent type '{}' for field '{}' (root: {})",
                    parent_type_name,
                    name,
                    parent_ctx.root_type
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
                let types = graphql_hir::schema_types(&self.db, project_files);
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
                let fragments = graphql_hir::all_fragments(&self.db, project_files);
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
            let registry = self.registry.read();

            let file_id = registry.get_file_id(file)?;

            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        let parse = graphql_syntax::parse(&self.db, content, metadata);

        let line_index = graphql_syntax::line_index(&self.db, content);

        let metadata_line_offset = metadata.line_offset(&self.db);
        let (block_context, adjusted_position) =
            find_block_for_position(&parse, position, metadata_line_offset)?;

        tracing::debug!(
            "Goto definition: original position {:?}, block line_offset {}, adjusted position {:?}",
            position,
            block_context.line_offset,
            adjusted_position
        );

        let offset = if let Some(block_source) = block_context.block_source {
            let block_line_index = graphql_syntax::LineIndex::new(block_source);
            position_to_offset(&block_line_index, adjusted_position)?
        } else {
            position_to_offset(&line_index, adjusted_position)?
        };

        let symbol = find_symbol_at_offset(block_context.tree, offset)?;

        let project_files = self.project_files?;

        match symbol {
            Symbol::FieldName { name } => {
                let parent_context = find_parent_type_at_offset(block_context.tree, offset)?;

                let schema_types = graphql_hir::schema_types(&self.db, project_files);

                // Use walk_type_stack_to_offset to properly resolve the parent type,
                // which handles inline fragments correctly
                let parent_type_name = symbol::walk_type_stack_to_offset(
                    block_context.tree,
                    &schema_types,
                    offset,
                    &parent_context.root_type,
                )?;

                tracing::debug!(
                    "Field '{}' - resolved parent type '{}' (root: {})",
                    name,
                    parent_type_name,
                    parent_context.root_type
                );

                schema_types.get(parent_type_name.as_str())?;

                let registry = self.registry.read();
                let schema_file_ids = project_files.schema_file_ids(&self.db).ids(&self.db);

                for file_id in schema_file_ids.iter() {
                    let Some(schema_content) = registry.get_content(*file_id) else {
                        continue;
                    };
                    let Some(schema_metadata) = registry.get_metadata(*file_id) else {
                        continue;
                    };
                    let Some(file_path) = registry.get_path(*file_id) else {
                        continue;
                    };

                    let schema_parse =
                        graphql_syntax::parse(&self.db, schema_content, schema_metadata);
                    let schema_line_index = graphql_syntax::line_index(&self.db, schema_content);
                    let schema_line_offset = schema_metadata.line_offset(&self.db);

                    if schema_parse.blocks.is_empty() {
                        // Pure GraphQL schema file
                        if let Some(ranges) = find_field_definition_full_range(
                            &schema_parse.tree,
                            &parent_type_name,
                            &name,
                        ) {
                            let range = offset_range_to_range(
                                &schema_line_index,
                                ranges.name_start,
                                ranges.name_end,
                            );
                            let adjusted_range =
                                adjust_range_for_line_offset(range, schema_line_offset);
                            return Some(vec![Location::new(file_path, adjusted_range)]);
                        }
                    } else {
                        // TS/JS file with embedded schema (unlikely but handle it)
                        for block in &schema_parse.blocks {
                            if let Some(ranges) = find_field_definition_full_range(
                                &block.tree,
                                &parent_type_name,
                                &name,
                            ) {
                                let block_line_index =
                                    graphql_syntax::LineIndex::new(&block.source);
                                let range = offset_range_to_range(
                                    &block_line_index,
                                    ranges.name_start,
                                    ranges.name_end,
                                );
                                #[allow(clippy::cast_possible_truncation)]
                                let block_line_offset = block.line as u32;
                                let adjusted_range =
                                    adjust_range_for_line_offset(range, block_line_offset);
                                return Some(vec![Location::new(file_path, adjusted_range)]);
                            }
                        }
                    }
                }

                // Field definition not found
                None
            }
            Symbol::FragmentSpread { name } => {
                let fragments = graphql_hir::all_fragments(&self.db, project_files);

                tracing::debug!(
                    "Looking for fragment '{}', available fragments: {:?}",
                    name,
                    fragments.keys().collect::<Vec<_>>()
                );

                let fragment = fragments.get(name.as_str())?;

                let registry = self.registry.read();

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

                let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);
                let def_line_offset = def_metadata.line_offset(&self.db);

                let range = find_fragment_definition_in_parse(
                    &def_parse,
                    &name,
                    def_content,
                    &self.db,
                    def_line_offset,
                )?;

                Some(vec![Location::new(file_path, range)])
            }
            Symbol::TypeName { name } => {
                let schema_ids = project_files.schema_file_ids(&self.db).ids(&self.db);
                let registry = self.registry.read();

                for file_id in schema_ids.iter() {
                    let Some(schema_content) = registry.get_content(*file_id) else {
                        continue;
                    };
                    let Some(schema_metadata) = registry.get_metadata(*file_id) else {
                        continue;
                    };
                    let Some(file_path) = registry.get_path(*file_id) else {
                        continue;
                    };

                    let schema_parse =
                        graphql_syntax::parse(&self.db, schema_content, schema_metadata);
                    let schema_line_offset = schema_metadata.line_offset(&self.db);

                    if let Some(range) = find_type_definition_in_parse(
                        &schema_parse,
                        &name,
                        schema_content,
                        &self.db,
                        schema_line_offset,
                    ) {
                        return Some(vec![Location::new(file_path, range)]);
                    }
                }

                // Type definition not found
                None
            }
            Symbol::VariableReference { name } => {
                let range = if let Some(block_source) = block_context.block_source {
                    let block_line_index = graphql_syntax::LineIndex::new(block_source);
                    find_variable_definition_in_tree(
                        block_context.tree,
                        &name,
                        &block_line_index,
                        block_context.line_offset,
                    )
                } else {
                    let file_line_index = graphql_syntax::line_index(&self.db, content);
                    find_variable_definition_in_tree(
                        block_context.tree,
                        &name,
                        &file_line_index,
                        block_context.line_offset,
                    )
                };

                if let Some(range) = range {
                    let registry = self.registry.read();
                    let file_id = registry.get_file_id(file)?;
                    let file_path = registry.get_path(file_id)?;
                    return Some(vec![Location::new(file_path, range)]);
                }
                None
            }
            Symbol::ArgumentName { name } => {
                let parent_context = find_parent_type_at_offset(block_context.tree, offset)?;
                let schema_types = graphql_hir::schema_types(&self.db, project_files);

                let field_name = find_field_name_at_offset(block_context.tree, offset)?;

                let parent_type_name = symbol::walk_type_stack_to_offset(
                    block_context.tree,
                    &schema_types,
                    offset,
                    &parent_context.root_type,
                )?;

                let registry = self.registry.read();
                let schema_file_ids = project_files.schema_file_ids(&self.db).ids(&self.db);

                for file_id in schema_file_ids.iter() {
                    let Some(schema_content) = registry.get_content(*file_id) else {
                        continue;
                    };
                    let Some(schema_metadata) = registry.get_metadata(*file_id) else {
                        continue;
                    };
                    let Some(file_path) = registry.get_path(*file_id) else {
                        continue;
                    };

                    let schema_parse =
                        graphql_syntax::parse(&self.db, schema_content, schema_metadata);
                    let schema_line_index = graphql_syntax::line_index(&self.db, schema_content);
                    let schema_line_offset = schema_metadata.line_offset(&self.db);

                    if let Some(range) = find_argument_definition_in_tree(
                        &schema_parse.tree,
                        &parent_type_name,
                        &field_name,
                        &name,
                        &schema_line_index,
                        schema_line_offset,
                    ) {
                        return Some(vec![Location::new(file_path, range)]);
                    }
                }
                None
            }
            Symbol::OperationName { name } => {
                let range = if let Some(block_source) = block_context.block_source {
                    let block_line_index = graphql_syntax::LineIndex::new(block_source);
                    find_operation_definition_in_tree(
                        block_context.tree,
                        &name,
                        &block_line_index,
                        block_context.line_offset,
                    )
                } else {
                    let file_line_index = graphql_syntax::line_index(&self.db, content);
                    find_operation_definition_in_tree(
                        block_context.tree,
                        &name,
                        &file_line_index,
                        block_context.line_offset,
                    )
                };

                if let Some(range) = range {
                    let registry = self.registry.read();
                    let file_id = registry.get_file_id(file)?;
                    let file_path = registry.get_path(file_id)?;
                    return Some(vec![Location::new(file_path, range)]);
                }
                None
            }
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
            let registry = self.registry.read();

            let file_id = registry.get_file_id(file)?;

            let content = registry.get_content(file_id)?;
            let metadata = registry.get_metadata(file_id)?;
            drop(registry);

            (content, metadata)
        };

        let parse = graphql_syntax::parse(&self.db, content, metadata);

        let line_index = graphql_syntax::line_index(&self.db, content);

        let metadata_line_offset = metadata.line_offset(&self.db);
        let (block_context, adjusted_position) =
            find_block_for_position(&parse, position, metadata_line_offset)?;

        tracing::debug!(
            "Find references: original position {:?}, block line_offset {}, adjusted position {:?}",
            position,
            block_context.line_offset,
            adjusted_position
        );

        let offset = if let Some(block_source) = block_context.block_source {
            let block_line_index = graphql_syntax::LineIndex::new(block_source);
            position_to_offset(&block_line_index, adjusted_position)?
        } else {
            position_to_offset(&line_index, adjusted_position)?
        };

        let symbol = find_symbol_at_offset(block_context.tree, offset)?;

        match symbol {
            Symbol::FragmentSpread { name } => {
                Some(self.find_fragment_references(&name, include_declaration))
            }
            Symbol::TypeName { name } => {
                Some(self.find_type_references(&name, include_declaration))
            }
            Symbol::FieldName { name } => {
                let parent_type = find_schema_field_parent_type(block_context.tree, offset)?;
                Some(self.find_field_references(&parent_type, &name, include_declaration))
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

        let Some(project_files) = self.project_files else {
            return locations;
        };

        let fragments = graphql_hir::all_fragments(&self.db, project_files);

        if include_declaration {
            if let Some(fragment) = fragments.get(fragment_name) {
                let registry = self.registry.read();
                let file_path = registry.get_path(fragment.file_id);
                let def_content = registry.get_content(fragment.file_id);
                let def_metadata = registry.get_metadata(fragment.file_id);
                drop(registry);

                if let (Some(file_path), Some(def_content), Some(def_metadata)) =
                    (file_path, def_content, def_metadata)
                {
                    let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);
                    let def_line_offset = def_metadata.line_offset(&self.db);

                    if let Some(range) = find_fragment_definition_in_parse(
                        &def_parse,
                        fragment_name,
                        def_content,
                        &self.db,
                        def_line_offset,
                    ) {
                        locations.push(Location::new(file_path, range));
                    }
                }
            }
        }

        // Search through all document files for fragment spreads
        let doc_ids = project_files.document_file_ids(&self.db).ids(&self.db);

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };

            let registry = self.registry.read();
            let file_path = registry.get_path(*file_id);
            drop(registry);

            let Some(file_path) = file_path else {
                continue;
            };

            let parse = graphql_syntax::parse(&self.db, content, metadata);
            let line_offset = metadata.line_offset(&self.db);

            let spread_ranges = find_fragment_spreads_in_parse(
                &parse,
                fragment_name,
                content,
                &self.db,
                line_offset,
            );

            for range in spread_ranges {
                locations.push(Location::new(file_path.clone(), range));
            }
        }

        locations
    }

    /// Find all references to a type
    fn find_type_references(&self, type_name: &str, include_declaration: bool) -> Vec<Location> {
        let mut locations = Vec::new();

        let Some(project_files) = self.project_files else {
            return locations;
        };

        let types = graphql_hir::schema_types(&self.db, project_files);

        if include_declaration {
            if let Some(type_def) = types.get(type_name) {
                let registry = self.registry.read();
                let file_path = registry.get_path(type_def.file_id);
                let def_content = registry.get_content(type_def.file_id);
                let def_metadata = registry.get_metadata(type_def.file_id);
                drop(registry);

                if let (Some(file_path), Some(def_content), Some(def_metadata)) =
                    (file_path, def_content, def_metadata)
                {
                    let def_parse = graphql_syntax::parse(&self.db, def_content, def_metadata);
                    let def_line_offset = def_metadata.line_offset(&self.db);

                    if let Some(range) = find_type_definition_in_parse(
                        &def_parse,
                        type_name,
                        def_content,
                        &self.db,
                        def_line_offset,
                    ) {
                        locations.push(Location::new(file_path, range));
                    }
                }
            }
        }

        let schema_ids = project_files.schema_file_ids(&self.db).ids(&self.db);

        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };

            let registry = self.registry.read();
            let file_path = registry.get_path(*file_id);
            drop(registry);

            let Some(file_path) = file_path else {
                continue;
            };

            let parse = graphql_syntax::parse(&self.db, content, metadata);
            let line_offset = metadata.line_offset(&self.db);

            let type_ranges =
                find_type_references_in_parse(&parse, type_name, content, &self.db, line_offset);

            for range in type_ranges {
                locations.push(Location::new(file_path.clone(), range));
            }
        }

        locations
    }

    /// Find all references to a field on a specific type
    fn find_field_references(
        &self,
        type_name: &str,
        field_name: &str,
        include_declaration: bool,
    ) -> Vec<Location> {
        let mut locations = Vec::new();

        let Some(project_files) = self.project_files else {
            return locations;
        };

        let schema_types = graphql_hir::schema_types(&self.db, project_files);

        if include_declaration {
            let schema_ids = project_files.schema_file_ids(&self.db).ids(&self.db);

            for file_id in schema_ids.iter() {
                let Some((content, metadata)) =
                    graphql_db::file_lookup(&self.db, project_files, *file_id)
                else {
                    continue;
                };

                let registry = self.registry.read();
                let file_path = registry.get_path(*file_id);
                drop(registry);

                let Some(file_path) = file_path else {
                    continue;
                };

                let parse = graphql_syntax::parse(&self.db, content, metadata);
                let line_index = graphql_syntax::line_index(&self.db, content);
                let line_offset = metadata.line_offset(&self.db);

                if let Some(ranges) =
                    find_field_definition_full_range(&parse.tree, type_name, field_name)
                {
                    let range =
                        offset_range_to_range(&line_index, ranges.name_start, ranges.name_end);
                    let adjusted_range = adjust_range_for_line_offset(range, line_offset);
                    locations.push(Location::new(file_path, adjusted_range));
                    break; // Field definition found
                }
            }
        }

        // Search through all document files for field usages
        let doc_ids = project_files.document_file_ids(&self.db).ids(&self.db);

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };

            let registry = self.registry.read();
            let file_path = registry.get_path(*file_id);
            drop(registry);

            let Some(file_path) = file_path else {
                continue;
            };

            let parse = graphql_syntax::parse(&self.db, content, metadata);
            let line_offset = metadata.line_offset(&self.db);

            let field_ranges = find_field_usages_in_parse(
                &parse,
                type_name,
                field_name,
                &schema_types,
                content,
                &self.db,
                line_offset,
            );

            for range in field_ranges {
                locations.push(Location::new(file_path.clone(), range));
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
            let registry = self.registry.read();

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

        let parse = graphql_syntax::parse(&self.db, content, metadata);
        let line_index = graphql_syntax::line_index(&self.db, content);

        let line_offset = metadata.line_offset(&self.db);

        let structure = graphql_hir::file_structure(&self.db, file_id, content, metadata);

        let mut symbols = Vec::new();

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
                    let children = get_field_children(
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
                    let children = get_field_children(
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
                    let children = get_field_children(
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
        let types = graphql_hir::schema_types(&self.db, project_files);
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
        let fragments = graphql_hir::all_fragments(&self.db, project_files);
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
        let doc_ids = project_files.document_file_ids(&self.db).ids(&self.db);
        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };
            let structure = graphql_hir::file_structure(&self.db, *file_id, content, metadata);
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
        let registry = self.registry.read();
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
        let registry = self.registry.read();
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

        let registry = self.registry.read();
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

/// Result of finding which block contains a position
struct BlockContext<'a> {
    /// The syntax tree for the block (or main document)
    tree: &'a apollo_parser::SyntaxTree,
    /// Line offset to add when returning positions (0 for pure GraphQL files)
    line_offset: u32,
    /// The block source for building `LineIndex` (None for pure GraphQL files)
    block_source: Option<&'a str>,
}

/// Get field children for a type definition
fn get_field_children(
    structure: &graphql_hir::FileStructureData,
    type_name: &str,
    tree: &apollo_parser::SyntaxTree,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Vec<DocumentSymbol> {
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

/// Find which GraphQL block contains the given position
///
/// For pure GraphQL files, returns the main tree with `line_offset` from metadata.
/// For TS/JS files, finds the block containing the position and returns it with
/// the appropriate line offset.
#[allow(clippy::cast_possible_truncation)]
fn find_block_for_position(
    parse: &graphql_syntax::Parse,
    position: Position,
    metadata_line_offset: u32,
) -> Option<(BlockContext<'_>, Position)> {
    // If no blocks, this is a pure GraphQL file - use main tree
    if parse.blocks.is_empty() {
        let adjusted_pos = adjust_position_for_line_offset(position, metadata_line_offset)?;
        return Some((
            BlockContext {
                tree: &parse.tree,
                line_offset: metadata_line_offset,
                block_source: None,
            },
            adjusted_pos,
        ));
    }

    // For TS/JS files, find which block contains the position
    for block in &parse.blocks {
        let block_start_line = block.line as u32;
        let block_start_col = block.column as u32;
        // Calculate block end line by counting newlines in source
        let block_lines = block.source.chars().filter(|&c| c == '\n').count() as u32;
        let block_end_line = block_start_line + block_lines;

        if position.line >= block_start_line && position.line <= block_end_line {
            // Position is within this block
            // Adjust position to be relative to block start (subtract both line and column)
            let adjusted_line = position.line - block_start_line;
            let adjusted_col = if adjusted_line == 0 {
                // Only subtract column offset on the first line of the block
                position.character.saturating_sub(block_start_col)
            } else {
                position.character
            };
            let adjusted_pos = Position::new(adjusted_line, adjusted_col);

            return Some((
                BlockContext {
                    tree: &block.tree,
                    line_offset: block_start_line,
                    block_source: Some(&block.source),
                },
                adjusted_pos,
            ));
        }
    }

    None
}

/// Find a fragment definition in a parsed file, handling TS/JS blocks correctly
///
/// For pure GraphQL files, searches the main tree.
/// For TS/JS files, searches each block and returns the correct position with block offset applied.
#[allow(clippy::cast_possible_truncation)]
fn find_fragment_definition_in_parse(
    parse: &graphql_syntax::Parse,
    fragment_name: &str,
    content: graphql_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
    metadata_line_offset: u32,
) -> Option<Range> {
    // For pure GraphQL files, search the main tree
    if parse.blocks.is_empty() {
        if let Some((start_offset, end_offset)) =
            find_fragment_definition_range(&parse.tree, fragment_name)
        {
            let file_line_index = graphql_syntax::line_index(db, content);
            let range = offset_range_to_range(&file_line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, metadata_line_offset));
        }
        return None;
    }

    for block in &parse.blocks {
        if let Some((start_offset, end_offset)) =
            find_fragment_definition_range(&block.tree, fragment_name)
        {
            let block_line_index = graphql_syntax::LineIndex::new(&block.source);
            let range = offset_range_to_range(&block_line_index, start_offset, end_offset);

            return Some(adjust_range_for_line_offset(range, block.line as u32));
        }
    }

    None
}

/// Find a type definition in a parsed file, handling TS/JS blocks correctly
#[allow(clippy::cast_possible_truncation)]
fn find_type_definition_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    content: graphql_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
    metadata_line_offset: u32,
) -> Option<Range> {
    // For pure GraphQL files, search the main tree
    if parse.blocks.is_empty() {
        if let Some((start_offset, end_offset)) = find_type_definition_range(&parse.tree, type_name)
        {
            let file_line_index = graphql_syntax::line_index(db, content);
            let range = offset_range_to_range(&file_line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, metadata_line_offset));
        }
        return None;
    }

    // For TS/JS files, search each block
    for block in &parse.blocks {
        if let Some((start_offset, end_offset)) = find_type_definition_range(&block.tree, type_name)
        {
            let block_line_index = graphql_syntax::LineIndex::new(&block.source);
            let range = offset_range_to_range(&block_line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, block.line as u32));
        }
    }

    None
}

/// Find all fragment spreads in a parsed file, handling TS/JS blocks correctly
///
/// Returns a vector of ranges with correct positions for each block.
#[allow(clippy::cast_possible_truncation)]
fn find_fragment_spreads_in_parse(
    parse: &graphql_syntax::Parse,
    fragment_name: &str,
    content: graphql_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
    metadata_line_offset: u32,
) -> Vec<Range> {
    let mut results = Vec::new();

    // For pure GraphQL files, search the main tree
    if parse.blocks.is_empty() {
        if let Some(offsets) = find_fragment_spreads(&parse.tree, fragment_name) {
            let file_line_index = graphql_syntax::line_index(db, content);
            for offset in offsets {
                let end_offset = offset + fragment_name.len();
                let range = offset_range_to_range(&file_line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, metadata_line_offset));
            }
        }
        return results;
    }

    // For TS/JS files, search each block
    for block in &parse.blocks {
        if let Some(offsets) = find_fragment_spreads(&block.tree, fragment_name) {
            let block_line_index = graphql_syntax::LineIndex::new(&block.source);
            for offset in offsets {
                let end_offset = offset + fragment_name.len();
                let range = offset_range_to_range(&block_line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, block.line as u32));
            }
        }
    }

    results
}

/// Find all type references in a parsed file, handling TS/JS blocks correctly
#[allow(clippy::cast_possible_truncation)]
fn find_type_references_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    content: graphql_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
    metadata_line_offset: u32,
) -> Vec<Range> {
    let mut results = Vec::new();

    // For pure GraphQL files, search the main tree
    if parse.blocks.is_empty() {
        if let Some(offsets) = find_type_references_in_tree(&parse.tree, type_name) {
            let file_line_index = graphql_syntax::line_index(db, content);
            for offset in offsets {
                let end_offset = offset + type_name.len();
                let range = offset_range_to_range(&file_line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, metadata_line_offset));
            }
        }
        return results;
    }

    // For TS/JS files, search each block
    for block in &parse.blocks {
        if let Some(offsets) = find_type_references_in_tree(&block.tree, type_name) {
            let block_line_index = graphql_syntax::LineIndex::new(&block.source);
            for offset in offsets {
                let end_offset = offset + type_name.len();
                let range = offset_range_to_range(&block_line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, block.line as u32));
            }
        }
    }

    results
}

/// Find field usages in a parsed file that match the given type and field name
#[allow(clippy::cast_possible_truncation)]
fn find_field_usages_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    field_name: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
    content: graphql_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
    metadata_line_offset: u32,
) -> Vec<Range> {
    let mut results = Vec::new();

    // For pure GraphQL files, search the main tree
    if parse.blocks.is_empty() {
        let file_line_index = graphql_syntax::line_index(db, content);
        let ranges = find_field_usages_in_tree(&parse.tree, type_name, field_name, schema_types);
        for (start, end) in ranges {
            let range = offset_range_to_range(&file_line_index, start, end);
            results.push(adjust_range_for_line_offset(range, metadata_line_offset));
        }
        return results;
    }

    // For TS/JS files, search each block
    for block in &parse.blocks {
        let block_line_index = graphql_syntax::LineIndex::new(&block.source);
        let ranges = find_field_usages_in_tree(&block.tree, type_name, field_name, schema_types);
        for (start, end) in ranges {
            let range = offset_range_to_range(&block_line_index, start, end);
            results.push(adjust_range_for_line_offset(range, block.line as u32));
        }
    }

    results
}

/// Check if `current_type` matches `target_type` directly or implements it as an interface
fn type_matches_or_implements(
    current_type: &str,
    target_type: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> bool {
    if current_type == target_type {
        return true;
    }
    if let Some(type_def) = schema_types.get(current_type) {
        type_def
            .implements
            .iter()
            .any(|i| i.as_ref() == target_type)
    } else {
        false
    }
}

/// Find all field usages in a tree that match the given type and field name
#[allow(clippy::too_many_lines)]
fn find_field_usages_in_tree(
    tree: &apollo_parser::SyntaxTree,
    target_type: &str,
    target_field: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> Vec<(usize, usize)> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn search_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        current_type: &str,
        target_type: &str,
        target_field: &str,
        schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
        results: &mut Vec<(usize, usize)>,
    ) {
        for selection in selection_set.selections() {
            match selection {
                Selection::Field(field) => {
                    if let Some(name) = field.name() {
                        let field_name = name.text();

                        if type_matches_or_implements(current_type, target_type, schema_types)
                            && field_name == target_field
                        {
                            let range = name.syntax().text_range();
                            results.push((range.start().into(), range.end().into()));
                        }

                        // Recurse into nested selection sets with the field's return type
                        if let Some(nested) = field.selection_set() {
                            if let Some(type_def) = schema_types.get(current_type) {
                                if let Some(field_def) = type_def
                                    .fields
                                    .iter()
                                    .find(|f| f.name.as_ref() == field_name)
                                {
                                    let field_type = field_def.type_ref.name.as_ref();
                                    search_selection_set(
                                        &nested,
                                        field_type,
                                        target_type,
                                        target_field,
                                        schema_types,
                                        results,
                                    );
                                }
                            }
                        }
                    }
                }
                Selection::InlineFragment(inline_frag) => {
                    let fragment_type = inline_frag
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| current_type.to_string(), |n| n.text().to_string());

                    if let Some(nested) = inline_frag.selection_set() {
                        search_selection_set(
                            &nested,
                            &fragment_type,
                            target_type,
                            target_field,
                            schema_types,
                            results,
                        );
                    }
                }
                Selection::FragmentSpread(_) => {
                    // Fragment definitions are searched separately in the main loop,
                    // so fields inside fragments will be found when we iterate over
                    // FragmentDefinition. We don't need to follow the spread here.
                }
            }
        }
    }

    let mut results = Vec::new();
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                let root_type = match op.operation_type() {
                    Some(op_type) if op_type.mutation_token().is_some() => "Mutation",
                    Some(op_type) if op_type.subscription_token().is_some() => "Subscription",
                    _ => "Query",
                };

                if let Some(selection_set) = op.selection_set() {
                    search_selection_set(
                        &selection_set,
                        root_type,
                        target_type,
                        target_field,
                        schema_types,
                        &mut results,
                    );
                }
            }
            Definition::FragmentDefinition(frag) => {
                let fragment_type = frag
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|n| n.text().to_string());

                if let (Some(fragment_type), Some(selection_set)) =
                    (fragment_type, frag.selection_set())
                {
                    search_selection_set(
                        &selection_set,
                        &fragment_type,
                        target_type,
                        target_field,
                        schema_types,
                        &mut results,
                    );
                }
            }
            _ => {}
        }
    }

    results
}

/// Find variable definition in an operation by name
fn find_variable_definition_in_tree(
    tree: &apollo_parser::SyntaxTree,
    var_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        if let Definition::OperationDefinition(op) = definition {
            if let Some(var_defs) = op.variable_definitions() {
                for var_def in var_defs.variable_definitions() {
                    if let Some(variable) = var_def.variable() {
                        if let Some(name) = variable.name() {
                            if name.text() == var_name {
                                let range = name.syntax().text_range();
                                let start: usize = range.start().into();
                                let end: usize = range.end().into();
                                let pos_range = offset_range_to_range(line_index, start, end);
                                return Some(adjust_range_for_line_offset(pos_range, line_offset));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find operation definition by name
fn find_operation_definition_in_tree(
    tree: &apollo_parser::SyntaxTree,
    op_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        if let Definition::OperationDefinition(op) = definition {
            if let Some(name) = op.name() {
                if name.text() == op_name {
                    let range = name.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    let pos_range = offset_range_to_range(line_index, start, end);
                    return Some(adjust_range_for_line_offset(pos_range, line_offset));
                }
            }
        }
    }
    None
}

/// Find argument definition in schema type's field
fn find_argument_definition_in_tree(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
    field_name: &str,
    arg_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        let (name_node, fields_def) = match &definition {
            Definition::ObjectTypeDefinition(obj) => (obj.name(), obj.fields_definition()),
            Definition::InterfaceTypeDefinition(iface) => (iface.name(), iface.fields_definition()),
            _ => continue,
        };

        let Some(name) = name_node else { continue };
        if name.text() != type_name {
            continue;
        }

        let Some(fields) = fields_def else { continue };
        for field in fields.field_definitions() {
            let Some(fname) = field.name() else { continue };
            if fname.text() != field_name {
                continue;
            }

            // Found the field, now find the argument
            if let Some(args_def) = field.arguments_definition() {
                for input_val in args_def.input_value_definitions() {
                    if let Some(aname) = input_val.name() {
                        if aname.text() == arg_name {
                            let range = aname.syntax().text_range();
                            let start: usize = range.start().into();
                            let end: usize = range.end().into();
                            let pos_range = offset_range_to_range(line_index, start, end);
                            return Some(adjust_range_for_line_offset(pos_range, line_offset));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find the field name at a given offset (for argument context)
fn find_field_name_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<String> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn check_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        byte_offset: usize,
    ) -> Option<String> {
        for selection in selection_set.selections() {
            if let Selection::Field(field) = selection {
                let range = field.syntax().text_range();
                let start: usize = range.start().into();
                let end: usize = range.end().into();

                if byte_offset >= start && byte_offset <= end {
                    if let Some(args) = field.arguments() {
                        let args_range = args.syntax().text_range();
                        let args_start: usize = args_range.start().into();
                        let args_end: usize = args_range.end().into();
                        if byte_offset >= args_start && byte_offset <= args_end {
                            return field.name().map(|n| n.text().to_string());
                        }
                    }

                    if let Some(nested) = field.selection_set() {
                        if let Some(name) = check_selection_set(&nested, byte_offset) {
                            return Some(name);
                        }
                    }
                }
            }
        }
        None
    }

    let doc = tree.document();
    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    if let Some(name) = check_selection_set(&selection_set, byte_offset) {
                        return Some(name);
                    }
                }
            }
            Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    if let Some(name) = check_selection_set(&selection_set, byte_offset) {
                        return Some(name);
                    }
                }
            }
            _ => {}
        }
    }
    None
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
#[allow(clippy::needless_raw_string_hashes)]
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
    fn test_extract_cursor_single_line() {
        let (source, pos) = extract_cursor("query { user*Name }");
        assert_eq!(source, "query { userName }");
        assert_eq!(pos, Position::new(0, 12));
    }

    #[test]
    fn test_extract_cursor_multiline() {
        let (source, pos) = extract_cursor("query {\n  user*Name\n}");
        assert_eq!(source, "query {\n  userName\n}");
        assert_eq!(pos, Position::new(1, 6));
    }

    #[test]
    fn test_extract_cursor_start_of_line() {
        let (source, pos) = extract_cursor("query {\n*  userName\n}");
        assert_eq!(source, "query {\n  userName\n}");
        assert_eq!(pos, Position::new(1, 0));
    }

    #[test]
    fn test_extract_cursor_graphql_example() {
        let input = r#"
fragment AttackActionInfo on AttackAction {
    pokemon {
        *...TeamPokemonBasic
    }
}
"#;
        let (source, pos) = extract_cursor(input);
        assert!(!source.contains('*'));
        assert_eq!(pos.line, 3);
        assert_eq!(pos.character, 8);
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
    fn test_diagnostics_after_file_update() {
        // This test verifies that file updates work correctly with Salsa's
        // incremental computation. Key insight from Salsa SME consultation:
        //
        // Salsa uses a single-writer, multi-reader model. When we clone the
        // IdeDatabase (via snapshot()), we create a snapshot that shares the
        // underlying storage. Salsa setters require exclusive access, so ALL
        // snapshots must be dropped before calling any setter (like set_text).
        //
        // The fix is to properly scope snapshot lifetimes: get diagnostics
        // inside a block so the snapshot is dropped before mutation.

        let mut host = AnalysisHost::new();

        // Add a file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, "type Query { hello: String }", FileKind::Schema, 0);

        // Get initial diagnostics - snapshot is scoped to this block
        let diagnostics1 = {
            let snapshot = host.snapshot();
            snapshot.diagnostics(&path)
        }; // snapshot dropped here, before mutation

        // Update the file - safe because no snapshots exist
        host.add_file(&path, "type Query { world: Int }", FileKind::Schema, 0);

        // Get new diagnostics - new snapshot for updated content
        let diagnostics2 = {
            let snapshot = host.snapshot();
            snapshot.diagnostics(&path)
        };

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
    fn test_hover_field_in_inline_fragment() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { battleParticipant(id: ID!): BattleParticipant }\ninterface BattleParticipant { id: ID! name: String! displayName: String! }\ntype BattlePokemon implements BattleParticipant { id: ID! name: String! displayName: String! currentHP: Int! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor(
            "query { battleParticipant(id: \"1\") { id name ... on BattlePokemon { current*HP } } }",
        );
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hover = snapshot.hover(&query_file, cursor_pos);

        assert!(
            hover.is_some(),
            "Should show hover info for field in inline fragment type"
        );
        let hover = hover.unwrap();
        assert!(hover.contents.contains("currentHP"));
        assert!(hover.contents.contains("Int!"));
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
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID }",
            FileKind::Schema,
            0,
        );

        // Add a fragment that references User
        let fragment_file = FilePath::new("file:///fragment.graphql");
        let (fragment_text, cursor_pos) = extract_cursor("fragment F on U*ser { id }");
        host.add_file(
            &fragment_file,
            &fragment_text,
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&fragment_file, cursor_pos);

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
        let (query_text, cursor_pos) = extract_cursor("query { u*ser }");
        dbg!(&query_text);
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

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
        let (query_text, cursor_pos) = extract_cursor("query { user { na*me } }");
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

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
        let (schema_text, cursor_pos) =
            extract_cursor("type Query { user: U*ser }\ntype User { id: ID! }");
        host.add_file(&schema_file, &schema_text, FileKind::Schema, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

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
    fn test_goto_definition_field_in_inline_fragment() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { battleParticipant(id: ID!): BattleParticipant }\ninterface BattleParticipant { id: ID! name: String! displayName: String! }\ntype BattlePokemon implements BattleParticipant { id: ID! name: String! displayName: String! currentHP: Int! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor(
            "query { battleParticipant(id: \"1\") { id name ... on BattlePokemon { current*HP } } }",
        );
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find field definition in inline fragment type"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "currentHP" field in BattlePokemon type (line 2)
        assert_eq!(locations[0].range.start.line, 2);
    }

    #[test]
    fn test_goto_definition_variable_reference() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user(id: ID!): User }\ntype User { id: ID! name: String! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on $id in the argument value
        let (query_text, cursor_pos) =
            extract_cursor("query GetUser($id: ID!) { user(id: $i*d) { name } }");
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find variable definition from usage"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), query_file.as_str());
        // Should point to the variable name (id) in the definition
        // "query GetUser($" = 15 chars, and we point to "id" not "$id"
        assert_eq!(locations[0].range.start.line, 0);
        assert_eq!(locations[0].range.start.character, 15);
    }

    #[test]
    fn test_goto_definition_argument_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user(id: ID!, name: String): User }\ntype User { id: ID! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on "id" argument name in the query
        let (query_text, cursor_pos) = extract_cursor("query { user(i*d: \"123\") { id } }");
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find argument definition in schema"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "id" argument in Query.user field definition
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_operation_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { hello: String }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on the operation name "GetHello"
        let (query_text, cursor_pos) = extract_cursor("query GetH*ello { hello }");
        host.add_file(&query_file, &query_text, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(locations.is_some(), "Should find operation definition");
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), query_file.as_str());
        // Should point to the operation name in the same file
        assert_eq!(locations[0].range.start.line, 0);
        assert_eq!(locations[0].range.start.character, 6); // "query " = 6 chars
    }

    #[test]
    fn test_goto_definition_implements_interface() {
        let mut host = AnalysisHost::new();

        // Schema with interface and type that implements it
        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) =
            extract_cursor("interface Node { id: ID! }\ntype User implements No*de { id: ID! }");
        host.add_file(&schema_file, &schema_text, FileKind::Schema, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find interface definition from implements clause"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Node" interface definition on line 0
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_implements_multiple_interfaces() {
        let mut host = AnalysisHost::new();

        // Schema with multiple interfaces
        let schema_file = FilePath::new("file:///schema.graphql");
        let schema_text = r#"interface Node { id: ID! }
interface Timestamped { createdAt: String! }
type User implements Node & Timestamped { id: ID!, createdAt: String! }"#;
        host.add_file(&schema_file, schema_text, FileKind::Schema, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Test cursor on "Timestamped" in implements clause
        // Line 2: "type User implements Node & Timestamped { id: ID!, createdAt: String! }"
        // "type User implements Node & " = 28 chars, then "Timestamped"
        let cursor_pos = Position::new(2, 30);
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find Timestamped interface definition"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Timestamped" interface definition on line 1
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_interface_extends_interface() {
        let mut host = AnalysisHost::new();

        // Interface extending another interface (GraphQL supports this)
        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) = extract_cursor(
            "interface Node { id: ID! }\ninterface Entity implements No*de { id: ID!, name: String }",
        );
        host.add_file(&schema_file, &schema_text, FileKind::Schema, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find interface definition from interface implements clause"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Node" interface definition on line 0
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_type_extension_implements() {
        let mut host = AnalysisHost::new();

        // Type extension adding an interface
        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) = extract_cursor(
            "interface Node { id: ID! }\ntype User { name: String }\nextend type User implements No*de",
        );
        host.add_file(&schema_file, &schema_text, FileKind::Schema, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find interface definition from type extension implements clause"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Node" interface definition on line 0
        assert_eq!(locations[0].range.start.line, 0);
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
    fn test_find_references_field_in_queries() {
        let mut host = AnalysisHost::new();

        // Add a schema with a type that has a name field
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! name: String! }",
            FileKind::Schema,
            0,
        );

        // Add a query that uses the name field
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { id name } }",
            FileKind::ExecutableGraphQL,
            0,
        );

        // Add a fragment that also uses the name field
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { name }",
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        // Find references to "name" field on User type
        // Schema line 2: "type User { id: ID! name: String! }"
        // "type User { id: ID! " = 20 chars, so "name" starts at position 20
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(1, 20), false);

        assert!(
            locations.is_some(),
            "Should find field references in documents"
        );
        let locations = locations.unwrap();
        // Should find: query usage + fragment usage = 2
        assert_eq!(
            locations.len(),
            2,
            "Expected 2 usages (query + fragment), got {}",
            locations.len()
        );
    }

    #[test]
    fn test_find_references_field_with_declaration() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { name: String! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { name } }",
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        // Find references including declaration
        // Line 1: "type User { " = 12 chars, so "name" starts at position 12
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(1, 12), true);

        assert!(locations.is_some());
        let locations = locations.unwrap();
        // Should find: declaration + query usage = 2
        assert_eq!(locations.len(), 2);

        // Verify one location is in schema, one in query
        let schema_refs: Vec<_> = locations
            .iter()
            .filter(|l| l.file.as_str() == schema_file.as_str())
            .collect();
        let query_refs: Vec<_> = locations
            .iter()
            .filter(|l| l.file.as_str() == query_file.as_str())
            .collect();
        assert_eq!(schema_refs.len(), 1, "Should have 1 schema reference");
        assert_eq!(query_refs.len(), 1, "Should have 1 query reference");
    }

    #[test]
    fn test_find_references_field_nested() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { profile: Profile }\ntype Profile { bio: String! }",
            FileKind::Schema,
            0,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { profile { bio } } }",
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        // Find references to "bio" field on Profile type
        // Line 2: "type Profile { " = 15 chars, "bio" starts at 15
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(2, 15), false);

        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1, "Should find nested field usage");
    }

    #[test]
    fn test_find_references_field_via_interface() {
        let mut host = AnalysisHost::new();

        // Schema with interface and implementing type
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { node: Node }\ninterface Node { id: ID! }\ntype User implements Node { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        // Query that uses the field on the implementing type
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { node { ... on User { id } } }",
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        // Find references to "id" field on Node interface
        // Line 1: "interface Node { " = 17 chars, "id" starts at 17
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(1, 17), false);

        assert!(
            locations.is_some(),
            "Should find field references via interface"
        );
        let locations = locations.unwrap();
        // Should find the usage in the query (User implements Node, so User.id matches Node.id)
        assert_eq!(
            locations.len(),
            1,
            "Should find field usage via interface implementation"
        );
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
    fn test_battle_graphql_attack_action_pokemon_completions() {
        // Simulate a GraphQL file similar to battle.graphql
        let (graphql, cursor_pos) = extract_cursor(
            r#"
fragment AttackActionInfo on AttackAction {
    pokemon {
*        ...TeamPokemonBasic
    }
    move {
        ...MoveInfo
    }
    damage
    wasEffective
}
"#,
        );

        // Minimal schema for the test
        let schema = r#"
type AttackAction {
    pokemon: TeamPokemon
    move: Move
    damage: Int
    wasEffective: Boolean
}
type TeamPokemon {
    id: ID!
    name: String!
    hp: Int
}
type Move {
    id: ID!
    name: String!
}
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(&schema_path, schema, FileKind::Schema, 0);
        let gql_path = FilePath::new("file:///battle.graphql");
        host.add_file(&gql_path, &graphql, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let completions = snapshot
            .completions(&gql_path, cursor_pos)
            .unwrap_or_default();
        let labels: Vec<_> = completions.iter().map(|i| i.label.as_str()).collect();

        // Should only suggest fields of TeamPokemon, not AttackAction
        assert!(
            labels.contains(&"id"),
            "Should suggest 'id' for TeamPokemon: got {labels:?}"
        );
        assert!(
            labels.contains(&"name"),
            "Should suggest 'name' for TeamPokemon: got {labels:?}"
        );
        assert!(
            labels.contains(&"hp"),
            "Should suggest 'hp' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"damage"),
            "Should NOT suggest 'damage' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"move"),
            "Should NOT suggest 'move' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"pokemon"),
            "Should NOT suggest 'pokemon' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"wasEffective"),
            "Should NOT suggest 'wasEffective' for TeamPokemon: got {labels:?}"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_typescript_off_by_one_parent_completions() {
        let schema = r#"
type Query { allPokemon(region: Region!, limit: Int): PokemonConnection }
type PokemonConnection { nodes: [Pokemon!]! }
type Pokemon {
    id: ID!
    name: String!
    evolution: Evolution
}
type Evolution {
    evolvesTo: [EvolutionEdge]
}
type EvolutionEdge {
    pokemon: Pokemon
    requirement: Requirement
}
interface Requirement { }
type LevelRequirement implements Requirement { level: Int }
enum Region { KANTO JOHTO }
"#;

        // Test 1: Inside 'requirement' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(&schema_path, schema, FileKind::Schema, 0);

            let (graphql1, pos1) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
                        pokemon {
                            id
                            name
                        }
                        requirement {
                            ... on LevelRequirement {
*                                level
                            }
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let ts_path1 = FilePath::new("file:///test1.graphql");
            host.add_file(&ts_path1, &graphql1, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&ts_path1, pos1).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"level"),
                "Should suggest 'level' inside LevelRequirement: got {labels:?}"
            );
            assert!(
                !labels.contains(&"requirement"),
                "Should NOT suggest 'requirement' inside requirement: got {labels:?}"
            );
            assert!(
                !labels.contains(&"pokemon"),
                "Should NOT suggest 'pokemon' inside requirement: got {labels:?}"
            );
        }

        // Test 2: Inside 'evolvesTo' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(&schema_path, schema, FileKind::Schema, 0);

            let (graphql2, pos2) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
*                        pokemon {
                            id
                            name
                        }
                        requirement {
                            ... on LevelRequirement {
                                level
                            }
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let ts_path2 = FilePath::new("file:///test2.graphql");
            host.add_file(&ts_path2, &graphql2, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&ts_path2, pos2).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"pokemon"),
                "Should suggest 'pokemon' inside evolvesTo: got {labels:?}"
            );
            assert!(
                labels.contains(&"requirement"),
                "Should suggest 'requirement' inside evolvesTo: got {labels:?}"
            );
            assert!(
                !labels.contains(&"evolvesTo"),
                "Should NOT suggest 'evolvesTo' inside evolvesTo: got {labels:?}"
            );
            assert!(
                !labels.contains(&"evolvesFrom"),
                "Should NOT suggest 'evolvesFrom' inside evolvesTo: got {labels:?}"
            );
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_typescript_deeply_nested_completions() {
        let schema = r#"
type Query { allPokemon(region: Region!, limit: Int): PokemonConnection }
type PokemonConnection { nodes: [Pokemon!]! }
type Pokemon {
    id: ID!
    name: String!
    evolution: Evolution
}
type Evolution {
    evolvesTo: [EvolutionEdge]
}
type EvolutionEdge {
    pokemon: Pokemon
    requirement: Requirement
}
interface Requirement { }
type LevelRequirement implements Requirement { level: Int }
enum Region { KANTO JOHTO }
"#;

        // Test completions inside 'evolution' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(&schema_path, schema, FileKind::Schema, 0);

            let (graphql1, pos1) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
*                    evolvesTo {
                        pokemon {
                            id
                            name
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let path1 = FilePath::new("file:///test1.graphql");
            host.add_file(&path1, &graphql1, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&path1, pos1).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"evolvesTo"),
                "Should suggest 'evolvesTo' inside evolution: got {labels:?}"
            );
        }

        // Test completions inside 'evolvesTo' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(&schema_path, schema, FileKind::Schema, 0);

            let (graphql2, pos2) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
*                        pokemon {
                            id
                            name
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let path2 = FilePath::new("file:///test2.graphql");
            host.add_file(&path2, &graphql2, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&path2, pos2).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"pokemon"),
                "Should suggest 'pokemon' inside evolvesTo: got {labels:?}"
            );
            assert!(
                labels.contains(&"requirement"),
                "Should suggest 'requirement' inside evolvesTo: got {labels:?}"
            );
        }

        // Test completions inside 'requirement' selection set with inline fragment
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(&schema_path, schema, FileKind::Schema, 0);

            let (graphql3, pos3) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
                        requirement {
                            ... on LevelRequirement {
*                                level
                            }
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let path3 = FilePath::new("file:///test3.graphql");
            host.add_file(&path3, &graphql3, FileKind::ExecutableGraphQL, 0);
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&path3, pos3).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"level"),
                "Should suggest 'level' inside requirement: got {labels:?}"
            );
        }
    }

    #[test]
    fn test_completions_for_union_type_suggest_inline_fragments() {
        let schema = r#"
type Query { evolution: EvolutionEdge }
type EvolutionEdge {
    pokemon: Pokemon
    requirement: EvolutionRequirement
}
type Pokemon { id: ID! name: String! }
union EvolutionRequirement = LevelRequirement | ItemRequirement | TradeRequirement | FriendshipRequirement
type LevelRequirement { level: Int }
type ItemRequirement { item: Item }
type TradeRequirement { withItem: Item }
type FriendshipRequirement { minimumFriendship: Int }
type Item { id: ID! name: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(&schema_path, schema, FileKind::Schema, 0);

        let (graphql, pos) = extract_cursor(
            r#"
query TestEvolution {
    evolution {
        requirement {
*
        }
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();

        // Should suggest inline fragments for union member types
        assert!(
            labels.contains(&"... on LevelRequirement"),
            "Should suggest '... on LevelRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on ItemRequirement"),
            "Should suggest '... on ItemRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on TradeRequirement"),
            "Should suggest '... on TradeRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on FriendshipRequirement"),
            "Should suggest '... on FriendshipRequirement' inline fragment: got {labels:?}"
        );

        // Should be Type kind
        for kind in kinds {
            assert_eq!(
                kind,
                CompletionKind::Type,
                "Union member completions should be Type kind"
            );
        }

        // Should NOT suggest any fields (unions have no fields)
        assert_eq!(
            labels.len(),
            4,
            "Should only suggest 4 union member types: got {labels:?}"
        );

        // Verify insert_text includes braces, newline, and cursor placeholder
        for item in &items {
            assert!(
                item.insert_text.is_some(),
                "Inline fragment should have insert_text"
            );
            let insert_text = item.insert_text.as_ref().unwrap();
            assert!(
                insert_text.contains("{\n  $0\n}"),
                "Insert text should contain braces with $0 cursor placeholder: got {insert_text}"
            );
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::Snippet),
                "Inline fragment should use snippet format"
            );
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_completions_for_interface_type_suggest_fields_and_inline_fragments() {
        let schema = r#"
type Query { evolution: EvolutionEdge }
type EvolutionEdge {
    pokemon: Pokemon
    requirement: Requirement
}
type Pokemon { id: ID! name: String! }
interface Requirement {
    description: String
}
type LevelRequirement implements Requirement {
    description: String
    level: Int
}
type ItemRequirement implements Requirement {
    description: String
    item: Item
}
type TradeRequirement implements Requirement {
    description: String
    withItem: Item
}
type Item { id: ID! name: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(&schema_path, schema, FileKind::Schema, 0);

        let (graphql, pos) = extract_cursor(
            r#"
query TestEvolution {
    evolution {
        requirement {
*
        }
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, FileKind::ExecutableGraphQL, 0);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        // Should suggest inline fragments for implementing types
        assert!(
            labels.contains(&"... on LevelRequirement"),
            "Should suggest '... on LevelRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on ItemRequirement"),
            "Should suggest '... on ItemRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on TradeRequirement"),
            "Should suggest '... on TradeRequirement' inline fragment: got {labels:?}"
        );

        // Should be 3 type suggestions (inline fragments) total
        let type_completions: Vec<_> = items
            .iter()
            .filter(|i| i.kind == CompletionKind::Type)
            .collect();
        assert_eq!(
            type_completions.len(),
            3,
            "Should suggest 3 inline fragment types: got {labels:?}"
        );

        // Should only suggest fields from the interface itself, not implementing types
        let field_completions: Vec<_> = items
            .iter()
            .filter(|i| i.kind == CompletionKind::Field)
            .collect();
        assert_eq!(
            field_completions.len(),
            1,
            "Should have 1 field completion from interface: got {labels:?}"
        );

        // Check interface field is suggested
        assert!(
            labels.contains(&"description"),
            "Should suggest 'description' from interface"
        );

        // Should NOT suggest fields specific to implementing types
        assert!(
            !labels.contains(&"level"),
            "Should NOT suggest 'level' (specific to LevelRequirement)"
        );
        assert!(
            !labels.contains(&"item"),
            "Should NOT suggest 'item' (specific to ItemRequirement)"
        );
        assert!(
            !labels.contains(&"withItem"),
            "Should NOT suggest 'withItem' (specific to TradeRequirement)"
        );

        // Verify inline fragment insert_text includes braces, newline, and cursor placeholder
        for item in type_completions {
            assert!(
                item.insert_text.is_some(),
                "Inline fragment should have insert_text"
            );
            let insert_text = item.insert_text.as_ref().unwrap();
            assert!(
                insert_text.contains("{\n  $0\n}"),
                "Insert text should contain braces with $0 cursor placeholder: got {insert_text}"
            );
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::Snippet),
                "Inline fragment should use snippet format"
            );
            // Verify sort_text is set to push inline fragments after fields
            assert!(
                item.sort_text.is_some(),
                "Inline fragment should have sort_text"
            );
            assert!(
                item.sort_text.as_ref().unwrap().starts_with("z_"),
                "Inline fragment sort_text should start with 'z_' to sort after fields: got {:?}",
                item.sort_text
            );
        }
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

    mod schema_loading {
        use super::*;
        use std::io::Write;

        #[test]
        fn test_load_typescript_schema() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript schema file
            let ts_schema_content = r#"
import { gql } from 'graphql-tag';

export const typeDefs = gql`
  type Query {
    user(id: ID!): User
  }

  type User {
    id: ID!
    name: String!
    email: String
  }
`;
"#;
            let ts_schema_path = temp_dir.path().join("schema.ts");
            let mut file = std::fs::File::create(&ts_schema_path).unwrap();
            file.write_all(ts_schema_content.as_bytes()).unwrap();

            // Create config
            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Path("schema.ts".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            };

            // Load schemas
            let mut host = AnalysisHost::new();
            host.set_extract_config(graphql_extract::ExtractConfig {
                allow_global_identifiers: false,
                ..Default::default()
            });
            let count = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 1 extracted schema from TS
            assert_eq!(
                count, 2,
                "Should load 2 schema files (builtins + extracted)"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the User type is available
            let symbols = snapshot.workspace_symbols("User");
            assert!(!symbols.is_empty(), "User type should be found");
            assert_eq!(symbols[0].name, "User");
        }

        #[test]
        fn test_load_typescript_schema_with_multiple_blocks() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript file with multiple GraphQL blocks
            let ts_content = r#"
import { gql } from 'graphql-tag';

export const types = gql`
  type Query {
    posts: [Post!]!
  }
`;

export const postType = gql`
  type Post {
    id: ID!
    title: String!
    content: String
  }
`;
"#;
            let ts_path = temp_dir.path().join("schema.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Path("schema.ts".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let count = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 2 extracted blocks
            assert_eq!(count, 3, "Should load 3 schema files (builtins + 2 blocks)");

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify both types are available
            let query_symbols = snapshot.workspace_symbols("Query");
            assert!(!query_symbols.is_empty(), "Query type should be found");

            let post_symbols = snapshot.workspace_symbols("Post");
            assert!(!post_symbols.is_empty(), "Post type should be found");
        }

        #[test]
        fn test_load_mixed_schema_files() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a pure GraphQL schema file
            let gql_content = r#"
type Query {
  users: [User!]!
}
"#;
            let gql_path = temp_dir.path().join("base.graphql");
            let mut file = std::fs::File::create(&gql_path).unwrap();
            file.write_all(gql_content.as_bytes()).unwrap();

            // Create a TypeScript schema extension
            let ts_content = r#"
import { gql } from 'graphql-tag';

export const typeDefs = gql`
  type User {
    id: ID!
    name: String!
  }
`;
"#;
            let ts_path = temp_dir.path().join("types.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            // Use multiple schema paths
            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Paths(vec![
                    "base.graphql".to_string(),
                    "types.ts".to_string(),
                ]),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let count = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 1 GraphQL file + 1 TS extraction
            assert_eq!(count, 3, "Should load 3 schema files");

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify both types are available
            let query_symbols = snapshot.workspace_symbols("Query");
            assert!(!query_symbols.is_empty(), "Query type should be found");

            let user_symbols = snapshot.workspace_symbols("User");
            assert!(
                !user_symbols.is_empty(),
                "User type should be found from TS file"
            );
        }

        #[test]
        fn test_load_typescript_schema_no_graphql_found() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript file without any GraphQL
            let ts_content = r#"
export const greeting = "Hello, World!";
export function greet(name: string) {
    return `Hello, ${name}!`;
}
"#;
            let ts_path = temp_dir.path().join("utils.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Path("utils.ts".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let count = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load Apollo client builtins (no GraphQL found in TS file)
            assert_eq!(count, 1, "Should only load builtins when no GraphQL found");
        }

        #[test]
        fn test_load_javascript_schema() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a JavaScript schema file
            let js_content = r#"
import { gql } from 'graphql-tag';

export const typeDefs = gql`
  type Query {
    product(id: ID!): Product
  }

  type Product {
    id: ID!
    name: String!
    price: Float!
  }
`;
"#;
            let js_path = temp_dir.path().join("schema.js");
            let mut file = std::fs::File::create(&js_path).unwrap();
            file.write_all(js_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Path("schema.js".to_string()),
                documents: None,
                include: None,
                exclude: None,
                lint: None,
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let count = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 1 extracted schema from JS
            assert_eq!(
                count, 2,
                "Should load 2 schema files (builtins + extracted)"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the Product type is available
            let symbols = snapshot.workspace_symbols("Product");
            assert!(!symbols.is_empty(), "Product type should be found");
        }
    }
}
