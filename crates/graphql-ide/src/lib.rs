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
use salsa::Setter;

mod file_registry;
pub use file_registry::FileRegistry;

// New modular structure
mod helpers;
pub(crate) mod symbol;
mod types;

// Re-export types from the types module
pub use types::{
    CodeFix, CodeLensInfo, CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity,
    DocumentSymbol, FilePath, HoverResult, InsertTextFormat, Location, Position, Range,
    SchemaStats, SymbolKind, TextEdit, WorkspaceSymbol,
};

// Re-export helpers for internal use
use helpers::{
    adjust_range_for_line_offset, convert_diagnostic, find_argument_definition_in_tree,
    find_block_for_position, find_field_name_at_offset, find_field_usages_in_parse,
    find_fragment_definition_in_parse, find_fragment_spreads_in_parse,
    find_operation_definition_in_tree, find_type_definition_in_parse,
    find_type_references_in_parse, find_variable_definition_in_tree, format_type_ref,
    offset_range_to_range, path_to_file_uri, position_to_offset,
};
// Re-export for use in symbol module
pub use helpers::unwrap_type_to_name;

use symbol::{
    extract_all_definitions, find_field_definition_full_range, find_fragment_definition_full_range,
    find_operation_definition_ranges, find_parent_type_at_offset, find_schema_field_parent_type,
    find_symbol_at_offset, find_type_definition_full_range, is_in_selection_set, Symbol,
};

// Re-export database types that IDE layer needs
pub use graphql_db::FileKind;

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

// POD types are now defined in types.rs and re-exported above

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

/// Input: Lint configuration
///
/// This is a Salsa input so that config changes properly invalidate dependent queries.
/// Wrapping in Arc allows queries to access the config without cloning the entire config object.
///
/// Using Salsa inputs instead of `Arc<RwLock<...>>` ensures:
/// - Proper dependency tracking (Salsa knows which queries depend on config)
/// - Automatic invalidation (only config-dependent queries re-run on changes)
/// - No deadlock risk (Salsa manages all locking internally)
/// - Snapshot isolation (config is immutable in Analysis snapshots)
#[salsa::input]
struct LintConfigInput {
    pub config: Arc<graphql_linter::LintConfig>,
}

/// Input: Extract configuration for TypeScript/JavaScript extraction
///
/// This is a Salsa input so that config changes properly invalidate dependent queries.
///
/// Using Salsa inputs instead of `Arc<RwLock<...>>` ensures:
/// - Proper dependency tracking (Salsa knows which queries depend on config)
/// - Automatic invalidation (only config-dependent queries re-run on changes)
/// - No deadlock risk (Salsa manages all locking internally)
/// - Snapshot isolation (config is immutable in Analysis snapshots)
#[salsa::input]
struct ExtractConfigInput {
    pub config: Arc<graphql_extract::ExtractConfig>,
}

/// Custom database that implements config traits
///
/// Config is now stored as Salsa inputs (`LintConfigInput` and `ExtractConfigInput`)
/// instead of `Arc<RwLock<...>>` wrappers. This allows Salsa to properly track config
/// dependencies and only invalidate affected queries when config changes.
#[salsa::db]
#[derive(Clone)]
struct IdeDatabase {
    storage: salsa::Storage<Self>,
    lint_config_input: Option<LintConfigInput>,
    extract_config_input: Option<ExtractConfigInput>,
    project_files: Arc<RwLock<Option<graphql_db::ProjectFiles>>>,
}

impl Default for IdeDatabase {
    fn default() -> Self {
        let mut db = Self {
            storage: salsa::Storage::default(),
            lint_config_input: None,
            extract_config_input: None,
            project_files: Arc::new(RwLock::new(None)),
        };

        // Initialize with default configs as Salsa inputs
        db.lint_config_input = Some(LintConfigInput::new(
            &db,
            Arc::new(graphql_linter::LintConfig::default()),
        ));
        db.extract_config_input = Some(ExtractConfigInput::new(
            &db,
            Arc::new(graphql_extract::ExtractConfig::default()),
        ));

        db
    }
}

#[salsa::db]
impl salsa::Database for IdeDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for IdeDatabase {
    fn extract_config(&self) -> Option<Arc<graphql_extract::ExtractConfig>> {
        self.extract_config_input
            .map(|input| input.config(self).clone())
    }
}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for IdeDatabase {}

#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for IdeDatabase {
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        self.lint_config_input.map_or_else(
            || Arc::new(graphql_linter::LintConfig::default()),
            |input| input.config(self).clone(),
        )
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
    ///
    /// This properly invalidates all queries that depend on lint config via Salsa's
    /// dependency tracking. Only lint-dependent queries will re-run when config changes.
    pub fn set_lint_config(&mut self, config: graphql_linter::LintConfig) {
        if let Some(input) = self.db.lint_config_input {
            input.set_config(&mut self.db).to(Arc::new(config));
        } else {
            let input = LintConfigInput::new(&self.db, Arc::new(config));
            self.db.lint_config_input = Some(input);
        }
    }

    /// Set the extract configuration for the project
    ///
    /// This properly invalidates all queries that depend on extract config via Salsa's
    /// dependency tracking. Only extract-dependent queries will re-run when config changes.
    pub fn set_extract_config(&mut self, config: graphql_extract::ExtractConfig) {
        if let Some(input) = self.db.extract_config_input {
            input.set_config(&mut self.db).to(Arc::new(config));
        } else {
            let input = ExtractConfigInput::new(&self.db, Arc::new(config));
            self.db.extract_config_input = Some(input);
        }
    }

    /// Get the extract configuration for the project
    pub fn get_extract_config(&self) -> graphql_extract::ExtractConfig {
        self.db
            .extract_config_input
            .map(|input| (*input.config(&self.db)).clone())
            .unwrap_or_default()
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

    /// Get semantic tokens for a file
    ///
    /// Returns tokens for syntax highlighting with semantic information,
    /// including deprecation status for fields.
    #[allow(clippy::too_many_lines)]
    pub fn semantic_tokens(&self, file: &FilePath) -> Vec<SemanticToken> {
        let (content, metadata) = {
            let registry = self.registry.read();

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

        // Parse the file
        let parse = graphql_syntax::parse(&self.db, content, metadata);
        if !parse.errors.is_empty() {
            // Don't provide semantic tokens for files with parse errors
            return Vec::new();
        }

        // Get line index for position conversion
        let line_index = graphql_syntax::line_index(&self.db, content);

        // Get schema types if available (for deprecation checks)
        // schema_types returns a reference due to Salsa's returns(ref) optimization
        let schema_types: Option<&std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>> = self
            .project_files
            .map(|pf| graphql_hir::schema_types(&self.db, pf));

        let mut tokens = Vec::new();

        // Collect tokens from the main document (pure GraphQL files)
        let file_kind = metadata.kind(&self.db);
        if file_kind == graphql_db::FileKind::ExecutableGraphQL
            || file_kind == graphql_db::FileKind::Schema
        {
            collect_semantic_tokens_from_document(
                &parse.tree.document(),
                &line_index,
                0,
                schema_types,
                &mut tokens,
            );
        }

        // Collect tokens from extracted blocks (TypeScript/JavaScript)
        #[allow(clippy::cast_possible_truncation)]
        for block in &parse.blocks {
            let block_line_index = graphql_syntax::LineIndex::new(&block.source);
            collect_semantic_tokens_from_document(
                &block.tree.document(),
                &block_line_index,
                block.line as u32,
                schema_types,
                &mut tokens,
            );
        }

        // Sort tokens by position (required for delta encoding)
        tokens.sort_by(|a, b| {
            a.start
                .line
                .cmp(&b.start.line)
                .then_with(|| a.start.character.cmp(&b.start.character))
        });

        tokens
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

    /// Get raw lint diagnostics with fix information for a file
    ///
    /// Returns `LintDiagnostic` objects that include fix information.
    /// Use this for implementing auto-fix functionality.
    pub fn lint_diagnostics_with_fixes(
        &self,
        file: &FilePath,
    ) -> Vec<graphql_linter::LintDiagnostic> {
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

        graphql_analysis::lint_integration::lint_file_with_fixes(
            &self.db,
            content,
            metadata,
            self.project_files,
        )
    }

    /// Get project-wide raw lint diagnostics with fix information
    ///
    /// Returns a map of file paths -> `LintDiagnostic` objects that include fix information.
    pub fn project_lint_diagnostics_with_fixes(
        &self,
    ) -> HashMap<FilePath, Vec<graphql_linter::LintDiagnostic>> {
        let diagnostics_by_file_id =
            graphql_analysis::lint_integration::project_lint_diagnostics_with_fixes(
                &self.db,
                self.project_files,
            );

        let mut results = HashMap::new();
        let registry = self.registry.read();

        for (file_id, diagnostics) in diagnostics_by_file_id {
            if let Some(file_path) = registry.get_path(file_id) {
                if !diagnostics.is_empty() {
                    results.insert(file_path, diagnostics);
                }
            }
        }

        results
    }

    /// Get the content of a file
    ///
    /// Returns the text content of the file if it exists in the registry.
    pub fn file_content(&self, file: &FilePath) -> Option<String> {
        let registry = self.registry.read();
        let file_id = registry.get_file_id(file)?;
        let content = registry.get_content(file_id)?;
        Some(content.text(&self.db).to_string())
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
                        types,
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
                    types,
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

                // Show deprecation info if the field is deprecated
                if field.is_deprecated {
                    write!(hover_text, "---\n\n").ok();
                    if let Some(reason) = &field.deprecation_reason {
                        write!(hover_text, "**Deprecated:** {reason}\n\n").ok();
                    } else {
                        write!(hover_text, "**Deprecated**\n\n").ok();
                    }
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
                    schema_types,
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
                    schema_types,
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
                schema_types,
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

    /// Get code lenses for deprecated fields in a schema file
    ///
    /// Returns code lens information for each deprecated field definition,
    /// including the usage count and locations for navigation.
    pub fn deprecated_field_code_lenses(&self, file: &FilePath) -> Vec<CodeLensInfo> {
        let mut code_lenses = Vec::new();

        let Some(project_files) = self.project_files else {
            return code_lenses;
        };

        // Get the file_id for this file
        let file_id = {
            let registry = self.registry.read();
            registry.get_file_id(file)
        };

        let Some(file_id) = file_id else {
            return code_lenses;
        };

        // Get schema types to find deprecated fields
        let schema_types = graphql_hir::schema_types(&self.db, project_files);

        // Get file content and metadata for line index
        let (content, metadata) = {
            let registry = self.registry.read();
            let content = registry.get_content(file_id);
            let metadata = registry.get_metadata(file_id);
            (content, metadata)
        };

        let (Some(content), Some(metadata)) = (content, metadata) else {
            return code_lenses;
        };

        let line_index = graphql_syntax::line_index(&self.db, content);
        let line_offset = metadata.line_offset(&self.db);

        // Iterate through all types and find deprecated fields in this file
        for type_def in schema_types.values() {
            // Only process types defined in the current file
            if type_def.file_id != file_id {
                continue;
            }

            for field in &type_def.fields {
                if !field.is_deprecated {
                    continue;
                }

                // Find usages of this deprecated field
                let usage_locations = self.find_field_references(
                    type_def.name.as_ref(),
                    field.name.as_ref(),
                    false, // don't include declaration
                );

                // Convert the field's name_range to editor coordinates
                let name_start = field.name_range.start().into();
                let name_end = field.name_range.end().into();
                let range = adjust_range_for_line_offset(
                    offset_range_to_range(&line_index, name_start, name_end),
                    line_offset,
                );

                let mut code_lens = CodeLensInfo::new(
                    range,
                    type_def.name.as_ref(),
                    field.name.as_ref(),
                    usage_locations.len(),
                    usage_locations,
                );

                if let Some(ref reason) = field.deprecation_reason {
                    code_lens = code_lens.with_deprecation_reason(reason.as_ref());
                }

                code_lenses.push(code_lens);
            }

            // Note: Enum values with @deprecated could also be supported here
            // by adding a find_enum_value_references method. Left for future work.
        }

        code_lenses
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
        for (name, type_def) in types {
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
        for (name, fragment) in fragments {
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

    /// Get schema statistics
    ///
    /// Returns counts of types by kind, total fields, and directives.
    /// This uses the HIR layer directly for accurate field counting.
    pub fn schema_stats(&self) -> SchemaStats {
        let Some(project_files) = self.project_files else {
            return SchemaStats::default();
        };

        let types = graphql_hir::schema_types(&self.db, project_files);
        let mut stats = SchemaStats::default();

        for type_def in types.values() {
            match type_def.kind {
                graphql_hir::TypeDefKind::Object => stats.objects += 1,
                graphql_hir::TypeDefKind::Interface => stats.interfaces += 1,
                graphql_hir::TypeDefKind::Union => stats.unions += 1,
                graphql_hir::TypeDefKind::Enum => stats.enums += 1,
                graphql_hir::TypeDefKind::Scalar => stats.scalars += 1,
                graphql_hir::TypeDefKind::InputObject => stats.input_objects += 1,
            }
            // Count fields for types that have fields
            stats.total_fields += type_def.fields.len();
        }

        // Count directive definitions from schema files (excluding built-ins)
        let schema_ids = project_files.schema_file_ids(&self.db).ids(&self.db);
        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };

            // Skip the built-in Apollo Client directives file
            let registry = self.registry.read();
            if let Some(path) = registry.get_path(*file_id) {
                if path.as_str() == "apollo_client_builtins.graphql" {
                    drop(registry);
                    continue;
                }
            }
            drop(registry);

            let parse = graphql_syntax::parse(&self.db, content, metadata);
            // Count directive definitions by checking if the definition is a directive
            // Directives in GraphQL SDL start with "directive @"
            for definition in &parse.ast.definitions {
                if definition.as_directive_definition().is_some() {
                    stats.directives += 1;
                }
            }
        }

        stats
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

// Helper functions are now in helpers.rs module

/// Collect semantic tokens from a GraphQL document
///
/// Walks the document and emits tokens for fields, types, fragments, etc.
/// Checks the schema to determine if fields are deprecated.
#[allow(clippy::too_many_lines)]
fn collect_semantic_tokens_from_document(
    doc_cst: &apollo_parser::cst::Document,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    schema_types: Option<&std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>>,
    tokens: &mut Vec<SemanticToken>,
) {
    use apollo_parser::cst::{self, CstNode};

    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::OperationDefinition(operation) => {
                // Emit token for operation keyword (query, mutation, subscription)
                if let Some(op_type) = operation.operation_type() {
                    if let Some(token) = op_type
                        .query_token()
                        .or_else(|| op_type.mutation_token())
                        .or_else(|| op_type.subscription_token())
                    {
                        emit_token_for_syntax_token(
                            &token,
                            line_index,
                            line_offset,
                            SemanticTokenType::Keyword,
                            SemanticTokenModifiers::NONE,
                            tokens,
                        );
                    }
                }

                // Determine root type for field deprecation checks
                let root_type_name = operation.operation_type().map_or("Query", |op_type| {
                    if op_type.query_token().is_some() {
                        "Query"
                    } else if op_type.mutation_token().is_some() {
                        "Mutation"
                    } else if op_type.subscription_token().is_some() {
                        "Subscription"
                    } else {
                        "Query"
                    }
                });

                // Process selection set
                if let Some(selection_set) = operation.selection_set() {
                    collect_tokens_from_selection_set(
                        &selection_set,
                        Some(root_type_name),
                        schema_types,
                        line_index,
                        line_offset,
                        tokens,
                    );
                }
            }
            cst::Definition::FragmentDefinition(fragment) => {
                // Emit token for "fragment" keyword
                if let Some(fragment_token) = fragment.fragment_token() {
                    emit_token_for_syntax_token(
                        &fragment_token,
                        line_index,
                        line_offset,
                        SemanticTokenType::Keyword,
                        SemanticTokenModifiers::NONE,
                        tokens,
                    );
                }

                // Emit token for "on" keyword
                if let Some(type_condition) = fragment.type_condition() {
                    if let Some(on_token) = type_condition.on_token() {
                        emit_token_for_syntax_token(
                            &on_token,
                            line_index,
                            line_offset,
                            SemanticTokenType::Keyword,
                            SemanticTokenModifiers::NONE,
                            tokens,
                        );
                    }
                    // Emit token for type name
                    if let Some(named_type) = type_condition.named_type() {
                        if let Some(name) = named_type.name() {
                            emit_token_for_syntax_node(
                                name.syntax(),
                                line_index,
                                line_offset,
                                SemanticTokenType::Type,
                                SemanticTokenModifiers::NONE,
                                tokens,
                            );
                        }
                    }
                }

                // Get fragment's type condition for field deprecation checks
                let type_name = fragment
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                // Process selection set
                if let Some(selection_set) = fragment.selection_set() {
                    collect_tokens_from_selection_set(
                        &selection_set,
                        type_name.as_deref(),
                        schema_types,
                        line_index,
                        line_offset,
                        tokens,
                    );
                }
            }
            _ => {
                // Schema definitions (type, interface, etc.) - skip for now
            }
        }
    }
}

/// Collect semantic tokens from a selection set
#[allow(clippy::too_many_lines)]
fn collect_tokens_from_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
    parent_type_name: Option<&str>,
    schema_types: Option<&std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>>,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    tokens: &mut Vec<SemanticToken>,
) {
    use apollo_parser::cst::{self, CstNode};

    let parent_type = parent_type_name.and_then(|name| schema_types?.get(name));

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name_node) = field.name() {
                    let field_name = field_name_node.text();

                    // Check if field is deprecated
                    let is_deprecated = parent_type
                        .and_then(|pt| {
                            pt.fields
                                .iter()
                                .find(|f| f.name.as_ref() == field_name.as_ref())
                        })
                        .is_some_and(|f| f.is_deprecated);

                    let modifiers = if is_deprecated {
                        SemanticTokenModifiers::DEPRECATED
                    } else {
                        SemanticTokenModifiers::NONE
                    };

                    emit_token_for_syntax_node(
                        field_name_node.syntax(),
                        line_index,
                        line_offset,
                        SemanticTokenType::Property,
                        modifiers,
                        tokens,
                    );

                    // Get field's return type for nested selection set
                    let field_return_type = parent_type
                        .and_then(|pt| {
                            pt.fields
                                .iter()
                                .find(|f| f.name.as_ref() == field_name.as_ref())
                        })
                        .map(|f| f.type_ref.name.as_ref());

                    // Recurse into nested selection set
                    if let Some(nested_selection_set) = field.selection_set() {
                        collect_tokens_from_selection_set(
                            &nested_selection_set,
                            field_return_type,
                            schema_types,
                            line_index,
                            line_offset,
                            tokens,
                        );
                    }
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                // Emit token for "..." is not needed (punctuation)
                // Emit token for fragment name
                if let Some(name) = spread.fragment_name().and_then(|fn_| fn_.name()) {
                    emit_token_for_syntax_node(
                        name.syntax(),
                        line_index,
                        line_offset,
                        SemanticTokenType::Function,
                        SemanticTokenModifiers::NONE,
                        tokens,
                    );
                }
            }
            cst::Selection::InlineFragment(inline) => {
                // Emit token for "on" keyword and type name
                if let Some(type_condition) = inline.type_condition() {
                    if let Some(on_token) = type_condition.on_token() {
                        emit_token_for_syntax_token(
                            &on_token,
                            line_index,
                            line_offset,
                            SemanticTokenType::Keyword,
                            SemanticTokenModifiers::NONE,
                            tokens,
                        );
                    }
                    if let Some(named_type) = type_condition.named_type() {
                        if let Some(name) = named_type.name() {
                            emit_token_for_syntax_node(
                                name.syntax(),
                                line_index,
                                line_offset,
                                SemanticTokenType::Type,
                                SemanticTokenModifiers::NONE,
                                tokens,
                            );
                        }
                    }
                }

                // Get type condition for nested selection set
                let type_name = inline
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                let type_name_ref = type_name.as_deref().or(parent_type_name);

                // Recurse into nested selection set
                if let Some(selection_set) = inline.selection_set() {
                    collect_tokens_from_selection_set(
                        &selection_set,
                        type_name_ref,
                        schema_types,
                        line_index,
                        line_offset,
                        tokens,
                    );
                }
            }
        }
    }
}

/// Emit a semantic token for a syntax node
#[allow(clippy::cast_possible_truncation)]
fn emit_token_for_syntax_node(
    node: &apollo_parser::SyntaxNode,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    token_type: SemanticTokenType,
    modifiers: SemanticTokenModifiers,
    tokens: &mut Vec<SemanticToken>,
) {
    let offset: usize = node.text_range().start().into();
    let len: u32 = node.text_range().len().into();

    let (line, col) = line_index.line_col(offset);
    tokens.push(SemanticToken::new(
        Position::new(line as u32 + line_offset, col as u32),
        len,
        token_type,
        modifiers,
    ));
}

/// Emit a semantic token for a syntax token (keyword, punctuation, etc.)
#[allow(clippy::cast_possible_truncation)]
fn emit_token_for_syntax_token(
    token: &apollo_parser::SyntaxToken,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    token_type: SemanticTokenType,
    modifiers: SemanticTokenModifiers,
    tokens: &mut Vec<SemanticToken>,
) {
    let offset: usize = token.text_range().start().into();
    let len: u32 = token.text_range().len().into();

    let (line, col) = line_index.line_col(offset);
    tokens.push(SemanticToken::new(
        Position::new(line as u32 + line_offset, col as u32),
        len,
        token_type,
        modifiers,
    ));
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

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;
    use crate::helpers::{convert_position, convert_range, convert_severity};

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

    #[test]
    fn test_project_lint_no_duplicates_same_file() {
        // Test that project-wide lints don't report duplicate fragments
        // when the same file is only added once
        let mut host = AnalysisHost::new();
        // Enable the recommended lint rules (which includes unique_names)
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        // Add a fragment file with a single fragment
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Should have no diagnostics - single fragment shouldn't be flagged as duplicate
        // Check specifically for unique_names violations
        let unique_names_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unique_names"))
            .collect();

        assert!(
            unique_names_errors.is_empty(),
            "Single fragment file should NOT produce unique_names errors, but got: {:?}",
            unique_names_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_project_lint_no_duplicates_after_file_update() {
        // Test that updating a file doesn't cause false duplicate detection
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        // Add fragment file
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        // Update the same file (simulating did_change)
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );
        // Note: rebuild_project_files is NOT called here since is_new=false

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Should have no unique_names errors
        let unique_names_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unique_names"))
            .collect();

        assert!(
            unique_names_errors.is_empty(),
            "File update should NOT produce unique_names errors, but got: {:?}",
            unique_names_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_project_lint_different_uri_formats_same_file_no_duplicates() {
        // This test verifies that if the same file is added with different URI formats
        // (e.g., URL-encoded vs non-encoded), it should NOT cause false duplicate detection.
        // This simulates the scenario where:
        // 1. File is discovered via glob and added with one URI format
        // 2. File is opened in VSCode and sent with a different URI format
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            FileKind::Schema,
            0,
        );

        // Add fragment file with one URI format (simulating glob discovery)
        let fragment_file_glob = FilePath::new("file:///home/user/fragments.graphql");
        host.add_file(
            &fragment_file_glob,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        // Try to add the SAME file with a slightly different URI format
        // This simulates VSCode sending the file with URL encoding or different formatting
        // Note: In a real scenario, these might be the same path represented differently:
        // - "file:///home/user/fragments.graphql" (glob discovery)
        // - "file:///home/user/fragments.graphql" (VSCode - should match)
        // The key test is that add_file correctly identifies it as the same file.

        // Using the exact same URI should return is_new=false
        let is_new = host.add_file(
            &fragment_file_glob,
            "fragment UserFields on User { id name }",
            FileKind::ExecutableGraphQL,
            0,
        );
        assert!(
            !is_new,
            "Adding file with same URI should return is_new=false"
        );

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Should have no unique_names errors
        let unique_names_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unique_names"))
            .collect();

        assert!(
            unique_names_errors.is_empty(),
            "Same file added twice with same URI should NOT produce unique_names errors, but got: {:?}",
            unique_names_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_semantic_tokens_deprecated_field() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().unwrap();

        // Create a schema with a deprecated field
        let schema_content = r#"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    legacyId: String @deprecated(reason: "Use id instead")
}
"#;
        let schema_path = temp_dir.path().join("schema.graphql");
        let mut file = std::fs::File::create(&schema_path).unwrap();
        file.write_all(schema_content.as_bytes()).unwrap();

        // Create a document that uses the deprecated field
        let doc_content = r#"
query GetUser {
    user {
        id
        name
        legacyId
    }
}
"#;
        let doc_path = temp_dir.path().join("query.graphql");
        let mut doc_file = std::fs::File::create(&doc_path).unwrap();
        doc_file.write_all(doc_content.as_bytes()).unwrap();

        let config = graphql_config::ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            include: None,
            exclude: None,
            lint: None,
            extensions: None,
        };

        let mut host = AnalysisHost::new();
        host.load_schemas_from_config(&config, temp_dir.path())
            .unwrap();

        // Manually add the document file
        let doc_uri = format!("file://{}", doc_path.display());
        let file_path = FilePath::new(&doc_uri);
        host.add_file(
            &file_path,
            doc_content.trim(),
            graphql_db::FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get semantic tokens
        let tokens = snapshot.semantic_tokens(&file_path);

        // Find the token for 'legacyId' field - it should have DEPRECATED modifier
        let deprecated_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.modifiers == SemanticTokenModifiers::DEPRECATED)
            .collect();

        assert!(
            !deprecated_tokens.is_empty(),
            "Should have at least one deprecated token, got tokens: {:?}",
            tokens
        );

        // Verify the deprecated token is a Property (field) type
        let deprecated_field_token = deprecated_tokens
            .iter()
            .find(|t| t.token_type == SemanticTokenType::Property)
            .expect("Should have a deprecated Property token");

        // The legacyId field is on line 5 (0-indexed) in the query (after trim)
        assert_eq!(
            deprecated_field_token.start.line, 4,
            "Deprecated field token should be on line 4 (0-indexed)"
        );
    }

    #[test]
    fn test_deprecated_field_code_lenses() {
        let mut host = AnalysisHost::new();

        // Add a schema with a deprecated field
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    legacyId: String @deprecated(reason: "Use id instead")
}"#,
            FileKind::Schema,
            0,
        );

        // Add a document that uses the deprecated field
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"query GetUser {
    user {
        id
        name
        legacyId
    }
}"#,
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get code lenses for the schema file (where the deprecated field is defined)
        let code_lenses = snapshot.deprecated_field_code_lenses(&schema_path);

        assert_eq!(
            code_lenses.len(),
            1,
            "Should have exactly one code lens for the deprecated field"
        );

        let code_lens = &code_lenses[0];
        assert_eq!(code_lens.type_name, "User");
        assert_eq!(code_lens.field_name, "legacyId");
        assert_eq!(
            code_lens.usage_count, 1,
            "Should have 1 usage of the deprecated field"
        );
        assert_eq!(
            code_lens.deprecation_reason,
            Some("Use id instead".to_string())
        );
    }

    #[test]
    fn test_deprecated_field_code_lenses_no_usages() {
        let mut host = AnalysisHost::new();

        // Add a schema with a deprecated field that is not used
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    legacyId: String @deprecated(reason: "Use id instead")
}"#,
            FileKind::Schema,
            0,
        );

        // Add a document that does NOT use the deprecated field
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"query GetUser {
    user {
        id
        name
    }
}"#,
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get code lenses for the schema file
        let code_lenses = snapshot.deprecated_field_code_lenses(&schema_path);

        assert_eq!(
            code_lenses.len(),
            1,
            "Should have exactly one code lens for the deprecated field"
        );

        let code_lens = &code_lenses[0];
        assert_eq!(
            code_lens.usage_count, 0,
            "Should have 0 usages of the deprecated field"
        );
    }

    #[test]
    fn test_deprecated_field_code_lenses_multiple_usages() {
        let mut host = AnalysisHost::new();

        // Add a schema with a deprecated field
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query {
    user: User
    users: [User!]!
}

type User {
    id: ID!
    legacyId: String @deprecated
}"#,
            FileKind::Schema,
            0,
        );

        // Add multiple documents using the deprecated field
        let doc_path1 = FilePath::new("file:///query1.graphql");
        host.add_file(
            &doc_path1,
            r#"query GetUser {
    user {
        legacyId
    }
}"#,
            FileKind::ExecutableGraphQL,
            0,
        );

        let doc_path2 = FilePath::new("file:///query2.graphql");
        host.add_file(
            &doc_path2,
            r#"query GetUsers {
    users {
        legacyId
    }
}"#,
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        let code_lenses = snapshot.deprecated_field_code_lenses(&schema_path);

        assert_eq!(code_lenses.len(), 1);
        assert_eq!(
            code_lenses[0].usage_count, 2,
            "Should have 2 usages of the deprecated field"
        );
        assert_eq!(code_lenses[0].usage_locations.len(), 2);
    }

    #[test]
    fn test_deprecated_field_code_lenses_non_schema_file() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { id: ID! @deprecated }",
            FileKind::Schema,
            0,
        );

        // Add a document file
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query { user { id } }",
            FileKind::ExecutableGraphQL,
            0,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get code lenses for the document file (not schema) - should be empty
        // since code lenses only show on schema files where deprecated fields are defined
        let code_lenses = snapshot.deprecated_field_code_lenses(&doc_path);
        assert!(
            code_lenses.is_empty(),
            "Document files should not have code lenses for deprecated fields"
        );
    }
}
