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
use std::sync::Arc;

use parking_lot::RwLock;
use salsa::Setter;

mod file_registry;
pub use file_registry::FileRegistry;

// New modular structure
mod helpers;
pub(crate) mod symbol;
mod types;

// Feature modules
mod code_lenses;
mod completion;
mod folding_ranges;
mod goto_definition;
mod hover;
mod inlay_hints;
mod references;
mod selection_range;
mod semantic_tokens;
mod symbols;

// Re-export types from the types module
pub use types::{
    CodeFix, CodeLens, CodeLensCommand, CodeLensInfo, CompletionItem, CompletionKind, Diagnostic,
    DiagnosticSeverity, DocumentSymbol, FilePath, FoldingRange, FoldingRangeKind,
    FragmentReference, FragmentUsage, HoverResult, InlayHint, InlayHintKind, InsertTextFormat,
    Location, PendingIntrospection, Position, ProjectStatus, Range, SchemaContentError,
    SchemaLoadResult, SchemaStats, SelectionRange, SymbolKind, TextEdit, WorkspaceSymbol,
};

// Re-export helpers for internal use
use helpers::{adjust_range_for_line_offset, convert_diagnostic, offset_range_to_range};
// Re-export for use in symbol module and LSP
pub use helpers::{path_to_file_uri, unwrap_type_to_name};

use symbol::{find_fragment_definition_full_range, find_operation_definition_ranges};

// Re-export database types that IDE layer needs
pub use graphql_base_db::{DocumentKind, Language};

/// Information about a loaded file from document discovery
#[derive(Debug, Clone)]
pub struct LoadedFile {
    /// The file path (as a URI string)
    pub path: FilePath,
    /// The source language
    pub language: Language,
    /// The document kind
    pub document_kind: DocumentKind,
}

/// File data that has been read from disk but not yet registered.
/// Used to separate file I/O from lock acquisition.
#[derive(Debug)]
pub struct DiscoveredFile {
    /// The file path (as a URI string)
    pub path: FilePath,
    /// The file content
    pub content: String,
    /// The source language
    pub language: Language,
    /// The document kind
    pub document_kind: DocumentKind,
}

/// A content mismatch error found during file discovery.
///
/// This indicates a file's content doesn't match its expected `DocumentKind`
/// based on which config pattern matched it.
#[derive(Debug, Clone)]
pub struct ContentMismatchError {
    /// The pattern that matched this file
    pub pattern: String,
    /// Path to the file with mismatched content
    pub file_path: std::path::PathBuf,
    /// What kind was expected (based on config)
    pub expected: graphql_config::FileType,
    /// Names of definitions that don't belong
    pub unexpected_definitions: Vec<String>,
}

/// Result of file discovery, containing both files and any validation errors.
#[derive(Debug, Default)]
pub struct FileDiscoveryResult {
    /// Successfully discovered files
    pub files: Vec<DiscoveredFile>,
    /// Content mismatch errors found during discovery
    pub errors: Vec<ContentMismatchError>,
}

impl FileDiscoveryResult {
    /// Returns true if there are any content mismatch errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Discover and read document files from config without requiring any locks.
///
/// This function performs all file I/O upfront so that lock acquisition
/// for registration can be brief. Returns the file data ready for registration,
/// along with any content mismatch errors found.
///
/// Files in the `documents` config are expected to contain executable definitions
/// (operations, fragments). If schema definitions are found, an error is reported.
pub fn discover_document_files(
    config: &graphql_config::ProjectConfig,
    workspace_path: &std::path::Path,
) -> FileDiscoveryResult {
    let Some(documents_config) = &config.documents else {
        return FileDiscoveryResult::default();
    };

    let patterns: Vec<String> = documents_config
        .patterns()
        .into_iter()
        .map(std::string::ToString::to_string)
        .collect();

    let mut result = FileDiscoveryResult::default();

    for pattern in patterns {
        // Skip negation patterns
        if pattern.trim().starts_with('!') {
            continue;
        }

        let expanded_patterns = expand_braces(&pattern);

        for expanded_pattern in expanded_patterns {
            let full_pattern = workspace_path.join(&expanded_pattern);

            match glob::glob(&full_pattern.display().to_string()) {
                Ok(paths) => {
                    for entry in paths {
                        match entry {
                            Ok(path) if path.is_file() => {
                                // Skip node_modules
                                if path.components().any(|c| c.as_os_str() == "node_modules") {
                                    continue;
                                }

                                // Read file content
                                match std::fs::read_to_string(&path) {
                                    Ok(content) => {
                                        let path_str = path.display().to_string();
                                        let (language, document_kind) =
                                            determine_document_file_kind(&path_str, &content);
                                        let file_path = path_to_file_path(&path);

                                        // Validate content matches expected kind (Executable)
                                        // For TS/JS files, we need to extract GraphQL first
                                        let graphql_content = if language.requires_extraction() {
                                            // Extract and concatenate all GraphQL blocks
                                            let config = graphql_extract::ExtractConfig::default();
                                            graphql_extract::extract_from_source(
                                                &content, language, &config,
                                            )
                                            .unwrap_or_default()
                                            .iter()
                                            .map(|block| block.source.as_str())
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                        } else {
                                            content.clone()
                                        };

                                        // Check for schema definitions in document files
                                        if let Some(mismatch) =
                                            graphql_syntax::validate_content_matches_kind(
                                                &graphql_content,
                                                DocumentKind::Executable,
                                            )
                                        {
                                            let definitions = match mismatch {
                                                graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema { definitions } => definitions,
                                                graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable { .. } => Vec::new(),
                                            };
                                            result.errors.push(ContentMismatchError {
                                                pattern: pattern.clone(),
                                                file_path: path.clone(),
                                                expected: graphql_config::FileType::Document,
                                                unexpected_definitions: definitions,
                                            });
                                        }

                                        result.files.push(DiscoveredFile {
                                            path: file_path,
                                            content,
                                            language,
                                            document_kind,
                                        });
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to read file {}: {}",
                                            path.display(),
                                            e
                                        );
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!("Glob entry error: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Invalid glob pattern '{}': {}", expanded_pattern, e);
                }
            }
        }
    }

    result
}

/// Expand brace patterns like `{ts,tsx}` into multiple patterns
///
/// This is needed because the glob crate doesn't support brace expansion.
/// For example, `**/*.{ts,tsx}` expands to `["**/*.ts", "**/*.tsx"]`.
fn expand_braces(pattern: &str) -> Vec<String> {
    if let Some(start) = pattern.find('{') {
        if let Some(end) = pattern.find('}') {
            let before = &pattern[..start];
            let after = &pattern[end + 1..];
            let options = &pattern[start + 1..end];

            return options
                .split(',')
                .map(|opt| format!("{before}{opt}{after}"))
                .collect();
        }
    }

    vec![pattern.to_string()]
}

/// Check if a path has a given extension (case-insensitive)
fn has_extension(path: &str, ext: &str) -> bool {
    path.len() > ext.len()
        && path.as_bytes()[path.len() - ext.len()..].eq_ignore_ascii_case(ext.as_bytes())
}

/// Determine `FileKind` for a document file based on its path.
///
/// This is used for files loaded from the `documents` configuration.
/// - `.ts`/`.tsx` files → TypeScript
/// - `.js`/`.jsx` files → JavaScript
/// - `.graphql`/`.gql` files → `ExecutableGraphQL`
///
/// Note: Files from the `schema` configuration are always `Language::GraphQL, DocumentKind::Schema`,
/// regardless of their extension.
fn determine_document_file_kind(path: &str, _content: &str) -> (Language, DocumentKind) {
    if has_extension(path, ".ts") || has_extension(path, ".tsx") {
        (Language::TypeScript, DocumentKind::Executable)
    } else if has_extension(path, ".js") || has_extension(path, ".jsx") {
        (Language::JavaScript, DocumentKind::Executable)
    } else {
        (Language::GraphQL, DocumentKind::Executable)
    }
}

/// Convert a filesystem path to a `FilePath` (URI format)
fn path_to_file_path(path: &std::path::Path) -> FilePath {
    let uri_string = path_to_file_uri(path);
    FilePath::new(uri_string)
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
/// All configuration is now stored as Salsa inputs (`LintConfigInput`, `ExtractConfigInput`,
/// and `ProjectFiles`) instead of `Arc<RwLock<...>>` wrappers. This allows Salsa to properly
/// track config dependencies and only invalidate affected queries when inputs change.
///
/// Queries can access `project_files` via `db.project_files()` and Salsa will automatically
/// track dependencies when the query calls getters like `project_files.schema_file_ids(db)`.
#[salsa::db]
#[derive(Clone)]
struct IdeDatabase {
    storage: salsa::Storage<Self>,
    lint_config_input: Option<LintConfigInput>,
    extract_config_input: Option<ExtractConfigInput>,
    /// Project files input - stores the current `ProjectFiles` Salsa input directly.
    /// Unlike the old `Arc<RwLock<...>>` approach, this enables proper Salsa dependency
    /// tracking: queries that call `db.project_files()` and then access fields like
    /// `project_files.schema_file_ids(db)` will have their dependencies tracked.
    project_files_input: Option<graphql_base_db::ProjectFiles>,
}

impl Default for IdeDatabase {
    fn default() -> Self {
        let mut db = Self {
            storage: salsa::Storage::default(),
            lint_config_input: None,
            extract_config_input: None,
            project_files_input: None,
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
impl graphql_hir::GraphQLHirDatabase for IdeDatabase {
    fn project_files(&self) -> Option<graphql_base_db::ProjectFiles> {
        self.project_files_input
    }
}

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
/// host.add_file(&file, new_content, kind); // Safe: no snapshots exist
///
/// // WRONG: Holding snapshot across mutation
/// let snapshot = host.snapshot();
/// let result = snapshot.diagnostics(&file);
/// host.add_file(&file, new_content, kind); // HANGS: snapshot still alive!
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
    ///
    /// Returns `true` if this is a new file, `false` if it's an update to an existing file.
    ///
    /// **IMPORTANT**: Only call `rebuild_project_files()` when this returns `true` (new file).
    /// Content-only updates do NOT require rebuilding the project index.
    pub fn add_file(
        &mut self,
        path: &FilePath,
        content: &str,
        language: Language,
        document_kind: DocumentKind,
    ) -> bool {
        let mut registry = self.registry.write();
        let (_, _, _, is_new) =
            registry.add_file(&mut self.db, path, content, language, document_kind);
        is_new
    }

    /// Batch-add pre-discovered files to the host.
    ///
    /// This is more efficient than calling `add_file` in a loop because
    /// file I/O has already been done. The lock is only held briefly
    /// for registration, not during disk reads.
    ///
    /// Returns the list of `LoadedFile` structs for building indexes.
    pub fn add_discovered_files(&mut self, files: &[DiscoveredFile]) -> Vec<LoadedFile> {
        let mut registry = self.registry.write();
        let mut loaded = Vec::with_capacity(files.len());

        for file in files {
            registry.add_file(
                &mut self.db,
                &file.path,
                &file.content,
                file.language,
                file.document_kind,
            );
            loaded.push(LoadedFile {
                path: file.path.clone(),
                language: file.language,
                document_kind: file.document_kind,
            });
        }

        loaded
    }

    /// Rebuild the `ProjectFiles` index after adding/removing files
    ///
    /// This should be called after batch adding files to avoid O(n²) performance.
    /// It's relatively expensive as it iterates through all files, so avoid calling
    /// it in a loop.
    ///
    /// This method also syncs the `ProjectFiles` to the database so queries can
    /// access it via `db.project_files()`.
    pub fn rebuild_project_files(&mut self) {
        let mut registry = self.registry.write();
        registry.rebuild_project_files(&mut self.db);

        // Sync project_files from registry to database
        // This enables queries to access project_files via db.project_files()
        self.db.project_files_input = registry.project_files();
    }

    /// Add multiple files in batch, then rebuild the project index once
    ///
    /// This is the recommended way to load multiple files at once. It:
    /// 1. Adds all files to the registry without rebuilding
    /// 2. Rebuilds the project index once at the end
    ///
    /// This is O(n) instead of O(n²) compared to calling `add_file` + `rebuild_project_files`
    /// for each file individually.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use graphql_ide::{AnalysisHost, FilePath, Language, DocumentKind};
    ///
    /// let mut host = AnalysisHost::new();
    /// let files = vec![
    ///     (FilePath::new("file:///schema.graphql"), "type Query { hello: String }", Language::GraphQL, DocumentKind::Schema),
    ///     (FilePath::new("file:///query.graphql"), "query { hello }", Language::GraphQL, DocumentKind::Executable),
    /// ];
    /// host.add_files_batch(&files);
    /// ```
    pub fn add_files_batch(&mut self, files: &[(FilePath, &str, Language, DocumentKind)]) {
        let mut registry = self.registry.write();
        let mut any_new = false;

        for (path, content, language, document_kind) in files {
            let (_, _, _, is_new) =
                registry.add_file(&mut self.db, path, content, *language, *document_kind);
            any_new = any_new || is_new;
        }

        // Only rebuild if at least one file was new
        if any_new {
            registry.rebuild_project_files(&mut self.db);
            // Sync project_files from registry to database
            self.db.project_files_input = registry.project_files();
        }
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
        language: Language,
        document_kind: DocumentKind,
    ) -> (bool, Analysis) {
        // Single lock acquisition for both operations
        let mut registry = self.registry.write();
        let (_, _, _, is_new) =
            registry.add_file(&mut self.db, path, content, language, document_kind);

        // If this is a new file, rebuild the index before creating snapshot
        // This also syncs project_files to self.db.project_files_input
        if is_new {
            registry.rebuild_project_files(&mut self.db);
            // Sync project_files from registry to database
            self.db.project_files_input = registry.project_files();
        }

        let project_files = self.db.project_files_input;
        // Release the lock before creating the snapshot (no longer needed)
        drop(registry);

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
    /// - Collects remote introspection configs for async fetching by the caller
    ///
    /// Returns a [`SchemaLoadResult`] containing the count of loaded files and any
    /// pending introspection configurations that require async fetching.
    pub fn load_schemas_from_config(
        &mut self,
        config: &graphql_config::ProjectConfig,
        base_dir: &std::path::Path,
    ) -> anyhow::Result<SchemaLoadResult> {
        // Always include Apollo Client built-in directives first
        const APOLLO_CLIENT_BUILTINS: &str = include_str!("apollo_client_builtins.graphql");
        self.add_file(
            &FilePath::new("apollo_client_builtins.graphql".to_string()),
            APOLLO_CLIENT_BUILTINS,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let mut count = 1;
        let mut loaded_paths = Vec::new();
        let mut pending_introspections = Vec::new();
        let mut content_errors = Vec::new();

        let patterns: Vec<String> = match &config.schema {
            graphql_config::SchemaConfig::Path(s) => vec![s.clone()],
            graphql_config::SchemaConfig::Paths(arr) => arr.clone(),
            graphql_config::SchemaConfig::Introspection(introspection) => {
                // Collect introspection config for async fetching by the caller
                tracing::info!(
                    "Found remote schema introspection config: {}",
                    introspection.url
                );
                pending_introspections.push(PendingIntrospection::from_config(introspection));
                vec![]
            }
        };

        for pattern in patterns {
            // Collect URL patterns as pending introspections for async fetching
            if pattern.starts_with("http://") || pattern.starts_with("https://") {
                tracing::info!("Found remote schema URL: {}", pattern);
                pending_introspections.push(PendingIntrospection {
                    url: pattern,
                    headers: None,
                    timeout: None,
                    retry: None,
                });
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
                                        if lang.requires_extraction() {
                                            // Extract GraphQL from TS/JS file
                                            let extract_config = self.get_extract_config();
                                            match graphql_extract::extract_from_source(
                                                &content,
                                                lang,
                                                &extract_config,
                                            ) {
                                                Ok(blocks) => {
                                                    // Validate all blocks for executable definitions
                                                    let all_sources: String = blocks
                                                        .iter()
                                                        .map(|b| b.source.as_str())
                                                        .collect::<Vec<_>>()
                                                        .join("\n");
                                                    if let Some(mismatch) = graphql_syntax::validate_content_matches_kind(
                                                        &all_sources,
                                                        DocumentKind::Schema,
                                                    ) {
                                                        let definitions = match mismatch {
                                                            graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable { definitions } => definitions,
                                                            graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema { .. } => Vec::new(),
                                                        };
                                                        content_errors.push(SchemaContentError {
                                                            pattern: pattern.clone(),
                                                            file_path: entry.clone(),
                                                            unexpected_definitions: definitions,
                                                        });
                                                    }

                                                    if blocks.len() == 1 {
                                                        // Single block: store original TS/JS content
                                                        // so the syntax crate can handle extraction
                                                        // with proper line offsets
                                                        self.add_file(
                                                            &FilePath::new(file_uri.clone()),
                                                            &content,
                                                            lang,
                                                            DocumentKind::Schema,
                                                        );
                                                        count += 1;
                                                    } else {
                                                        // Multiple blocks: create separate entries
                                                        // with line range URIs for each block
                                                        for block in &blocks {
                                                            let start_line =
                                                                block.location.range.start.line + 1;
                                                            let end_line =
                                                                block.location.range.end.line + 1;
                                                            let block_uri = format!(
                                                                "{file_uri}#L{start_line}-L{end_line}"
                                                            );

                                                            self.add_file(
                                                                &FilePath::new(block_uri),
                                                                &block.source,
                                                                Language::GraphQL,
                                                                DocumentKind::Schema,
                                                            );
                                                            count += 1;
                                                        }
                                                    }
                                                    if blocks.is_empty() {
                                                        tracing::debug!(
                                                            "No GraphQL blocks found in {}",
                                                            entry.display()
                                                        );
                                                    } else {
                                                        loaded_paths.push(entry.clone());
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

                                    // Pure GraphQL file - validate and add
                                    // Check for executable definitions (operations/fragments)
                                    if let Some(mismatch) =
                                        graphql_syntax::validate_content_matches_kind(
                                            &content,
                                            DocumentKind::Schema,
                                        )
                                    {
                                        let definitions = match mismatch {
                                            graphql_syntax::ContentMismatch::ExpectedSchemaFoundExecutable { definitions } => definitions,
                                            graphql_syntax::ContentMismatch::ExpectedExecutableFoundSchema { .. } => Vec::new(),
                                        };
                                        content_errors.push(SchemaContentError {
                                            pattern: pattern.clone(),
                                            file_path: entry.clone(),
                                            unexpected_definitions: definitions,
                                        });
                                    }

                                    self.add_file(
                                        &FilePath::new(file_uri),
                                        &content,
                                        Language::GraphQL,
                                        DocumentKind::Schema,
                                    );
                                    loaded_paths.push(entry.clone());
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

        tracing::info!(
            "Loaded {} schema file(s) ({} paths tracked), {} pending introspection(s)",
            count,
            loaded_paths.len(),
            pending_introspections.len()
        );
        Ok(SchemaLoadResult {
            loaded_count: count,
            loaded_paths,
            pending_introspections,
            content_errors,
        })
    }

    /// Add an introspected schema as a virtual file.
    ///
    /// This method registers a schema fetched via introspection using a virtual URI
    /// in the format `schema://<host>/<path>/schema.graphql`. The `.graphql`
    /// extension ensures editors recognize the file for syntax highlighting and
    /// language features.
    ///
    /// # Arguments
    ///
    /// * `url` - The original GraphQL endpoint URL (used to generate the virtual URI)
    /// * `sdl` - The schema SDL obtained from introspection
    ///
    /// # Returns
    ///
    /// The virtual file URI used to register the schema.
    pub fn add_introspected_schema(&mut self, url: &str, sdl: &str) -> String {
        let virtual_uri = format!(
            "schema://{}/schema.graphql",
            url.trim_start_matches("https://")
                .trim_start_matches("http://")
        );
        tracing::info!("Adding introspected schema from {} as {}", url, virtual_uri);
        self.add_file(
            &FilePath::new(virtual_uri.clone()),
            sdl,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        virtual_uri
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

    /// Load document files from a project configuration
    ///
    /// This method handles:
    /// - Glob pattern expansion (including brace expansion like `{ts,tsx}`)
    /// - File reading and content extraction
    /// - File kind determination based on extension
    /// - Batch file registration (more efficient than individual `add_file` calls)
    ///
    /// Returns information about loaded files for indexing purposes.
    ///
    /// # Arguments
    ///
    /// * `config` - The project configuration containing document patterns
    /// * `workspace_path` - The base directory for glob pattern resolution
    ///
    /// # Returns
    ///
    /// A vector of `LoadedFile` structs containing file paths and metadata.
    /// The caller can use this information to build file-to-project indexes.
    pub fn load_documents_from_config(
        &mut self,
        config: &graphql_config::ProjectConfig,
        workspace_path: &std::path::Path,
    ) -> Vec<LoadedFile> {
        let Some(documents_config) = &config.documents else {
            return Vec::new();
        };

        let patterns: Vec<String> = documents_config
            .patterns()
            .into_iter()
            .map(std::string::ToString::to_string)
            .collect();

        let mut loaded_files: Vec<LoadedFile> = Vec::new();
        let mut files_to_add: Vec<(FilePath, String, Language, DocumentKind)> = Vec::new();

        for pattern in patterns {
            // Skip negation patterns
            if pattern.trim().starts_with('!') {
                continue;
            }

            let expanded_patterns = expand_braces(&pattern);

            for expanded_pattern in expanded_patterns {
                let full_pattern = workspace_path.join(&expanded_pattern);

                match glob::glob(&full_pattern.display().to_string()) {
                    Ok(paths) => {
                        for entry in paths {
                            match entry {
                                Ok(path) if path.is_file() => {
                                    // Skip node_modules
                                    if path.components().any(|c| c.as_os_str() == "node_modules") {
                                        continue;
                                    }

                                    // Read file content
                                    match std::fs::read_to_string(&path) {
                                        Ok(content) => {
                                            let path_str = path.display().to_string();
                                            let (language, document_kind) =
                                                determine_document_file_kind(&path_str, &content);

                                            let file_path = path_to_file_path(&path);

                                            loaded_files.push(LoadedFile {
                                                path: file_path.clone(),
                                                language,
                                                document_kind,
                                            });

                                            files_to_add.push((
                                                file_path,
                                                content,
                                                language,
                                                document_kind,
                                            ));
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Failed to read file {}: {}",
                                                path.display(),
                                                e
                                            );
                                        }
                                    }
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    tracing::warn!("Glob entry error: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Invalid glob pattern '{}': {}", expanded_pattern, e);
                    }
                }
            }
        }

        // Batch add all files using add_files_batch for O(n) performance
        // Convert owned strings to borrowed for the batch API
        let batch_refs: Vec<(FilePath, &str, Language, DocumentKind)> = files_to_add
            .iter()
            .map(|(path, content, language, document_kind)| {
                (path.clone(), content.as_str(), *language, *document_kind)
            })
            .collect();
        self.add_files_batch(&batch_refs);

        loaded_files
    }

    /// Iterate over all files in the host
    ///
    /// Returns an iterator of `FilePath` for all registered files.
    pub fn files(&self) -> Vec<FilePath> {
        let registry = self.registry.read();
        registry
            .all_file_ids()
            .into_iter()
            .filter_map(|file_id| registry.get_path(file_id))
            .collect()
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
        // project_files is already synced to the database in rebuild_project_files()
        // Queries access it via db.project_files()
        let project_files = self.db.project_files_input;

        if let Some(ref pf) = project_files {
            let doc_count = pf.document_file_ids(&self.db).ids(&self.db).len();
            let schema_count = pf.schema_file_ids(&self.db).ids(&self.db).len();
            tracing::debug!(
                "Snapshot project_files: {} schema files, {} document files",
                schema_count,
                doc_count
            );
        } else {
            tracing::warn!("Snapshot project_files is None!");
        }

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
    project_files: Option<graphql_base_db::ProjectFiles>,
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
    pub fn semantic_tokens(&self, file: &FilePath) -> Vec<SemanticToken> {
        let registry = self.registry.read();
        semantic_tokens::semantic_tokens(&self.db, &registry, self.project_files, file)
    }

    /// Get folding ranges for a file
    ///
    /// Returns foldable regions for:
    /// - Operation definitions (query, mutation, subscription)
    /// - Fragment definitions
    /// - Selection sets
    /// - Block comments
    pub fn folding_ranges(&self, file: &FilePath) -> Vec<FoldingRange> {
        let registry = self.registry.read();
        folding_ranges::folding_ranges(&self.db, &registry, file)
    }

    /// Get inlay hints for a file within an optional range.
    ///
    /// Returns inlay hints showing return types after scalar field selections.
    /// Includes support for the `__typename` introspection field.
    ///
    /// If `range` is provided, only returns hints within that range for efficiency.
    pub fn inlay_hints(&self, file: &FilePath, range: Option<Range>) -> Vec<InlayHint> {
        let registry = self.registry.read();
        inlay_hints::inlay_hints(&self.db, &registry, self.project_files, file, range)
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

    /// Get all diagnostics for all files, merging per-file and project-wide diagnostics
    ///
    /// This is a convenience method for publishing diagnostics. It:
    /// 1. Gets per-file diagnostics (parse errors, validation errors, per-file lint rules)
    /// 2. Gets project-wide lint diagnostics (unused fields, etc.)
    /// 3. Merges them per file
    ///
    /// Returns a map of file paths -> all diagnostics for that file.
    pub fn all_diagnostics(&self) -> HashMap<FilePath, Vec<Diagnostic>> {
        let mut results: HashMap<FilePath, Vec<Diagnostic>> = HashMap::new();

        // Get all registered files
        let all_file_paths: Vec<FilePath> = {
            let registry = self.registry.read();
            registry
                .all_file_ids()
                .into_iter()
                .filter_map(|file_id| registry.get_path(file_id))
                .collect()
        };

        // Get per-file diagnostics for all files
        for file_path in &all_file_paths {
            let per_file = self.diagnostics(file_path);
            if !per_file.is_empty() {
                results.insert(file_path.clone(), per_file);
            }
        }

        // Get project-wide diagnostics and merge
        let project_diagnostics = self.project_lint_diagnostics();
        for (file_path, diagnostics) in project_diagnostics {
            results.entry(file_path).or_default().extend(diagnostics);
        }

        results
    }

    /// Get all diagnostics for a single file, merging per-file and project-wide diagnostics
    ///
    /// This returns the complete set of diagnostics for a file:
    /// - Per-file diagnostics (parse errors, validation errors, per-file lint rules)
    /// - Project-wide diagnostics (`unused_fields`, etc.) that apply to this file
    ///
    /// Use this when publishing diagnostics to avoid overwriting project-wide diagnostics
    /// with only per-file diagnostics.
    pub fn all_diagnostics_for_file(&self, file: &FilePath) -> Vec<Diagnostic> {
        let mut results = self.diagnostics(file);

        // Add project-wide diagnostics for this file
        let project_diagnostics = self.project_lint_diagnostics();
        if let Some(project_diags) = project_diagnostics.get(file) {
            results.extend(project_diags.iter().cloned());
        }

        results
    }

    /// Get all diagnostics for a specific set of files, merging per-file and project-wide diagnostics
    ///
    /// This is useful when you want diagnostics for specific files (e.g., loaded document files)
    /// rather than all files in the registry.
    pub fn all_diagnostics_for_files(
        &self,
        files: &[FilePath],
    ) -> HashMap<FilePath, Vec<Diagnostic>> {
        let mut results: HashMap<FilePath, Vec<Diagnostic>> = HashMap::new();

        // Get per-file diagnostics for specified files
        for file_path in files {
            let per_file = self.diagnostics(file_path);
            if !per_file.is_empty() {
                results.insert(file_path.clone(), per_file);
            }
        }

        // Get project-wide diagnostics and merge
        let project_diagnostics = self.project_lint_diagnostics();
        for (file_path, diagnostics) in project_diagnostics {
            // Only include if the file is in our set OR it's a schema file with issues
            // (project-wide lints like unused_fields report on schema files)
            results.entry(file_path).or_default().extend(diagnostics);
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

    /// Get the status of the project (file counts, schema loaded, etc.)
    ///
    /// Returns status information for the LSP status command.
    #[must_use]
    pub fn project_status(&self) -> ProjectStatus {
        let Some(project_files) = self.project_files else {
            return ProjectStatus::default();
        };

        let schema_file_count = project_files.schema_file_ids(&self.db).ids(&self.db).len();
        let document_file_count = project_files
            .document_file_ids(&self.db)
            .ids(&self.db)
            .len();

        ProjectStatus::new(schema_file_count, document_file_count)
    }

    /// Get field usage coverage report for the project
    ///
    /// Analyzes which schema fields are used in operations and returns
    /// detailed coverage statistics. This is useful for understanding
    /// schema usage patterns and finding unused fields.
    pub fn field_coverage(&self) -> Option<FieldCoverageReport> {
        let pf = self.project_files?;
        Some(FieldCoverageReport::from(
            graphql_analysis::analyze_field_usage(&self.db, pf),
        ))
    }

    /// Get field usage for a specific field
    ///
    /// Returns usage information for a field if it exists in the schema.
    /// Useful for enhancing hover to show "Used in N operations".
    pub fn field_usage(&self, type_name: &str, field_name: &str) -> Option<FieldUsageInfo> {
        let pf = self.project_files?;
        let coverage = graphql_analysis::analyze_field_usage(&self.db, pf);
        let key = (
            std::sync::Arc::from(type_name),
            std::sync::Arc::from(field_name),
        );
        coverage.field_usages.get(&key).map(|usage| FieldUsageInfo {
            usage_count: usage.usage_count,
            operations: usage.operations.iter().map(ToString::to_string).collect(),
        })
    }

    /// Get complexity analysis for all operations in the project
    ///
    /// Analyzes each operation's selection set to calculate:
    /// - Total complexity score (with list multipliers)
    /// - Maximum depth
    /// - Per-field complexity breakdown
    /// - Connection pattern detection (Relay-style edges/nodes/pageInfo)
    /// - Warnings about potential issues (nested pagination, etc.)
    pub fn complexity_analysis(&self) -> Vec<ComplexityAnalysis> {
        let Some(project_files) = self.project_files else {
            return Vec::new();
        };

        // Get all operations in the project
        let operations = graphql_hir::all_operations(&self.db, project_files);
        let schema_types = graphql_hir::schema_types(&self.db, project_files);

        let mut results = Vec::new();

        for operation in operations.iter() {
            // Get file information for this operation
            let registry = self.registry.read();
            let Some(file_path) = registry.get_path(operation.file_id) else {
                continue;
            };
            let Some(content) = registry.get_content(operation.file_id) else {
                continue;
            };
            let Some(metadata) = registry.get_metadata(operation.file_id) else {
                continue;
            };
            drop(registry);

            // Get operation body
            let body = graphql_hir::operation_body(&self.db, content, metadata, operation.index);

            // Get operation location for the range
            let range = if let Some(ref name) = operation.name {
                let parse = graphql_syntax::parse(&self.db, content, metadata);
                let mut found_range = None;
                for doc in parse.documents() {
                    if let Some(ranges) = find_operation_definition_ranges(doc.tree, name) {
                        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                        let doc_line_offset = doc.line_offset;
                        found_range = Some(adjust_range_for_line_offset(
                            offset_range_to_range(
                                &doc_line_index,
                                ranges.def_start,
                                ranges.def_end,
                            ),
                            doc_line_offset,
                        ));
                        break;
                    }
                }
                found_range.unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)))
            } else {
                Range::new(Position::new(0, 0), Position::new(0, 0))
            };

            // Create complexity analysis
            let op_name = operation
                .name
                .as_ref()
                .map_or_else(|| "<anonymous>".to_string(), ToString::to_string);

            #[allow(clippy::match_same_arms)]
            let op_type = match operation.operation_type {
                graphql_hir::OperationType::Query => "query",
                graphql_hir::OperationType::Mutation => "mutation",
                graphql_hir::OperationType::Subscription => "subscription",
                _ => "query", // fallback for future operation types
            };

            let mut analysis = ComplexityAnalysis::new(op_name, op_type, file_path, range);

            // Get the root type for this operation
            #[allow(clippy::match_same_arms)]
            let root_type_name = match operation.operation_type {
                graphql_hir::OperationType::Query => "Query",
                graphql_hir::OperationType::Mutation => "Mutation",
                graphql_hir::OperationType::Subscription => "Subscription",
                _ => "Query", // fallback for future operation types
            };

            // Analyze the operation body
            analyze_selections(
                &body.selections,
                schema_types,
                root_type_name,
                "",
                0,
                1,
                &mut analysis,
                false,
            );

            results.push(analysis);
        }

        results
    }

    /// Get completions at a position
    ///
    /// Returns a list of completion items appropriate for the context.
    pub fn completions(&self, file: &FilePath, position: Position) -> Option<Vec<CompletionItem>> {
        let registry = self.registry.read();
        completion::completions(&self.db, &registry, self.project_files, file, position)
    }

    /// Get hover information at a position
    ///
    /// Returns documentation, type information, etc.
    pub fn hover(&self, file: &FilePath, position: Position) -> Option<HoverResult> {
        let registry = self.registry.read();
        hover::hover(&self.db, &registry, self.project_files, file, position)
    }

    /// Get goto definition locations for the symbol at a position
    ///
    /// Returns the definition location(s) for types, fields, fragments, etc.
    pub fn goto_definition(&self, file: &FilePath, position: Position) -> Option<Vec<Location>> {
        let registry = self.registry.read();
        goto_definition::goto_definition(&self.db, &registry, self.project_files, file, position)
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
        let registry = self.registry.read();
        references::find_references(
            &self.db,
            &registry,
            self.project_files,
            file,
            position,
            include_declaration,
        )
    }

    /// Find all references to a fragment
    pub fn find_fragment_references(
        &self,
        fragment_name: &str,
        include_declaration: bool,
    ) -> Vec<Location> {
        let registry = self.registry.read();
        references::find_fragment_references(
            &self.db,
            &registry,
            self.project_files,
            fragment_name,
            include_declaration,
        )
    }

    /// Get selection ranges for smart expand/shrink selection
    ///
    /// Returns a `SelectionRange` for each input position, forming a linked list
    /// from the innermost syntax element to the outermost (document).
    /// This powers the "Expand Selection" (Shift+Alt+Right) and
    /// "Shrink Selection" (Shift+Alt+Left) features.
    pub fn selection_ranges(
        &self,
        file: &FilePath,
        positions: &[Position],
    ) -> Vec<Option<SelectionRange>> {
        let registry = self.registry.read();
        selection_range::selection_ranges(&self.db, &registry, file, positions)
    }

    /// Get code lenses for deprecated fields in a schema file
    ///
    /// Returns code lens information for each deprecated field definition,
    /// including the usage count and locations for navigation.
    pub fn deprecated_field_code_lenses(&self, file: &FilePath) -> Vec<CodeLensInfo> {
        let registry = self.registry.read();
        code_lenses::deprecated_field_code_lenses(&self.db, &registry, self.project_files, file)
    }

    /// Get document symbols for a file (hierarchical outline)
    ///
    /// Returns types, operations, and fragments with their fields as children.
    /// This powers the "Go to Symbol in Editor" (Cmd+Shift+O) feature.
    pub fn document_symbols(&self, file: &FilePath) -> Vec<DocumentSymbol> {
        let registry = self.registry.read();
        symbols::document_symbols(&self.db, &registry, file)
    }

    /// Search for workspace symbols matching a query
    ///
    /// Returns matching types, operations, and fragments across all files.
    /// This powers the "Go to Symbol in Workspace" (Cmd+T) feature.
    pub fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        let registry = self.registry.read();
        symbols::workspace_symbols(&self.db, &registry, self.project_files, query)
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
                _ => {} // ignore future type kinds
            }
            // Count fields for types that have fields
            stats.total_fields += type_def.fields.len();
        }

        // Count directive definitions from schema files (excluding built-ins)
        let schema_ids = project_files.schema_file_ids(&self.db).ids(&self.db);
        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(&self.db, project_files, *file_id)
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
            for doc in parse.documents() {
                for definition in &doc.ast.definitions {
                    if definition.as_directive_definition().is_some() {
                        stats.directives += 1;
                    }
                }
            }
        }

        stats
    }

    /// Get fragment usage analysis for the project
    ///
    /// Returns information about each fragment: its definition location,
    /// all usages (fragment spreads), and transitive dependencies.
    pub fn fragment_usages(&self) -> Vec<FragmentUsage> {
        let Some(project_files) = self.project_files else {
            return Vec::new();
        };

        let fragments = graphql_hir::all_fragments(&self.db, project_files);
        let mut results = Vec::new();

        for (name, fragment) in fragments {
            // Get definition location
            let Some((def_file, def_range)) = self.get_fragment_def_info(fragment) else {
                continue;
            };

            // Get all usages (fragment spreads) excluding the definition
            let spread_locations = self.find_fragment_references(name, false);
            let usages: Vec<FragmentReference> = spread_locations
                .into_iter()
                .map(FragmentReference::new)
                .collect();

            // Get transitive dependencies using the fragment spreads index
            let transitive_deps = self.compute_transitive_dependencies(name, project_files);

            results.push(FragmentUsage {
                name: name.to_string(),
                definition_file: def_file,
                definition_range: def_range,
                usages,
                transitive_dependencies: transitive_deps,
            });
        }

        // Sort by name for consistent ordering
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    /// Get fragment definition file and range
    fn get_fragment_def_info(
        &self,
        fragment: &graphql_hir::FragmentStructure,
    ) -> Option<(FilePath, Range)> {
        let registry = self.registry.read();
        let file_path = registry.get_path(fragment.file_id)?;
        let content = registry.get_content(fragment.file_id)?;
        let metadata = registry.get_metadata(fragment.file_id)?;
        drop(registry);

        let parse = graphql_syntax::parse(&self.db, content, metadata);

        for doc in parse.documents() {
            if let Some(ranges) = find_fragment_definition_full_range(doc.tree, &fragment.name) {
                let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                let range = adjust_range_for_line_offset(
                    offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end),
                    doc.line_offset,
                );
                return Some((file_path, range));
            }
        }

        None
    }

    /// Compute transitive fragment dependencies
    fn compute_transitive_dependencies(
        &self,
        fragment_name: &str,
        project_files: graphql_base_db::ProjectFiles,
    ) -> Vec<String> {
        let spreads_index = graphql_hir::fragment_spreads_index(&self.db, project_files);

        let mut visited = std::collections::HashSet::new();
        let mut to_visit = Vec::new();

        // Start with direct dependencies
        if let Some(direct_deps) = spreads_index.get(fragment_name) {
            to_visit.extend(direct_deps.iter().cloned());
        }

        while let Some(dep_name) = to_visit.pop() {
            if !visited.insert(dep_name.clone()) {
                continue; // Already visited (handles cycles)
            }

            // Add transitive dependencies
            if let Some(nested_deps) = spreads_index.get(&dep_name) {
                for nested in nested_deps {
                    if !visited.contains(nested) {
                        to_visit.push(nested.clone());
                    }
                }
            }
        }

        let mut deps: Vec<String> = visited.into_iter().map(|s| s.to_string()).collect();
        deps.sort();
        deps
    }

    /// Get code lenses for a file
    ///
    /// Returns code lenses for fragment definitions showing reference counts.
    pub fn code_lenses(&self, file: &FilePath) -> Vec<CodeLens> {
        let fragment_usages = self.fragment_usages();
        let registry = self.registry.read();
        code_lenses::code_lenses(
            &self.db,
            &registry,
            self.project_files,
            file,
            &fragment_usages,
        )
    }
}

// Helper functions are now in feature modules

/// Analyze selections recursively to calculate complexity
#[allow(clippy::too_many_arguments)]
fn analyze_selections(
    selections: &[graphql_hir::Selection],
    schema_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    parent_type_name: &str,
    path_prefix: &str,
    depth: u32,
    multiplier: u32,
    analysis: &mut ComplexityAnalysis,
    in_connection: bool,
) {
    // Update max depth
    if depth > analysis.depth {
        analysis.depth = depth;
    }

    for selection in selections {
        match selection {
            graphql_hir::Selection::Field {
                name,
                selection_set,
                ..
            } => {
                let field_name = name.to_string();
                let path = if path_prefix.is_empty() {
                    field_name.clone()
                } else {
                    format!("{path_prefix}.{field_name}")
                };

                // Get field type info from schema
                let (is_list, inner_type_name) =
                    get_type_info(schema_types, parent_type_name, &field_name);

                // Calculate field multiplier
                let field_multiplier = if is_list {
                    multiplier * 10 // Default list multiplier
                } else {
                    multiplier
                };

                // Check for connection pattern
                let field_is_connection =
                    is_connection_pattern(&field_name, schema_types, &inner_type_name);

                // Warn about nested pagination
                if in_connection && field_is_connection {
                    analysis.warnings.push(format!(
                        "Nested pagination detected at {path}. This can cause performance issues."
                    ));
                }

                // Calculate complexity for this field
                let field_complexity = field_multiplier;
                analysis.total_complexity += field_complexity;

                // Add to breakdown
                let mut fc = FieldComplexity::new(&path, &field_name, field_complexity)
                    .with_multiplier(if is_list { 10 } else { 1 })
                    .with_depth(depth)
                    .with_connection(field_is_connection);

                if in_connection && field_is_connection {
                    fc = fc.with_warning("Nested pagination");
                }

                analysis.breakdown.push(fc);

                // Recurse into nested selections
                if !selection_set.is_empty() {
                    analyze_selections(
                        selection_set,
                        schema_types,
                        &inner_type_name,
                        &path,
                        depth + 1,
                        field_multiplier,
                        analysis,
                        field_is_connection || in_connection,
                    );
                }
            }
            graphql_hir::Selection::FragmentSpread { .. }
            | graphql_hir::Selection::InlineFragment { .. } => {
                // For simplicity, we don't deeply analyze fragment spreads in this implementation
                // A full implementation would resolve the fragment and analyze its selections
            }
        }
    }
}

/// Check if a field follows the Relay connection pattern (edges/nodes/pageInfo)
fn is_connection_pattern(
    _field_name: &str,
    schema_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    type_name: &str,
) -> bool {
    // Check if the return type has edges, nodes, or pageInfo fields
    if let Some(type_def) = schema_types.get(type_name) {
        if type_def.kind == graphql_hir::TypeDefKind::Object {
            let has_edges = type_def.fields.iter().any(|f| f.name.as_ref() == "edges");
            let has_page_info = type_def
                .fields
                .iter()
                .any(|f| f.name.as_ref() == "pageInfo");
            let has_nodes = type_def.fields.iter().any(|f| f.name.as_ref() == "nodes");

            return (has_edges || has_nodes) && has_page_info;
        }
    }
    false
}

/// Get type information for a field: (`is_list`, `inner_type_name`)
fn get_type_info(
    schema_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    parent_type_name: &str,
    field_name: &str,
) -> (bool, String) {
    if let Some(type_def) = schema_types.get(parent_type_name) {
        if type_def.kind == graphql_hir::TypeDefKind::Object {
            if let Some(field) = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == field_name)
            {
                let type_ref = &field.type_ref;
                return (type_ref.is_list, type_ref.name.to_string());
            }
        }
    }
    (false, "Unknown".to_string())
}

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;
    use crate::helpers::{convert_position, convert_range, convert_severity, position_to_offset};

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
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Get initial diagnostics - snapshot is scoped to this block
        let diagnostics1 = {
            let snapshot = host.snapshot();
            snapshot.diagnostics(&path)
        }; // snapshot dropped here, before mutation

        // Update the file - safe because no snapshots exist
        host.add_file(
            &path,
            "type Query { world: Int }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

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
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
        host.add_file(
            &path,
            "type Query {",
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
    fn test_hover_on_schema_field_definition() {
        let mut host = AnalysisHost::new();

        // Add a schema file with a type definition
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Pokemon {\n  name: String!\n  level: Int!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a document that uses this field
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetPokemon { pokemon { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Get hover on "name" field in the schema definition (line 1, col 2 = "name")
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&schema_path, Position::new(1, 2));

        // Should return hover information for the field
        assert!(hover.is_some(), "Expected hover on schema field definition");
        let hover = hover.unwrap();
        assert!(hover.contents.contains("Field"), "Should contain 'Field'");
        assert!(hover.contents.contains("name"), "Should contain field name");
        assert!(hover.contents.contains("String"), "Should contain type");
        // Field is used in one operation, so should show usage count
        assert!(
            hover.contents.contains("Used in"),
            "Should contain usage information"
        );
    }

    #[test]
    fn test_hover_on_schema_field_shows_unused() {
        let mut host = AnalysisHost::new();

        // Add a schema file with a type definition
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Pokemon {\n  name: String!\n  level: Int!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        host.rebuild_project_files();

        // Get hover on "level" field which is not used in any operation
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&schema_path, Position::new(2, 2));

        // Should show "0 operations (unused)"
        assert!(hover.is_some(), "Expected hover on schema field definition");
        let hover = hover.unwrap();
        assert!(
            hover.contents.contains("0 operations"),
            "Should indicate unused field"
        );
    }

    #[test]
    fn test_hover_field_in_inline_fragment() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { battleParticipant(id: ID!): BattleParticipant }\ninterface BattleParticipant { id: ID! name: String! displayName: String! }\ntype BattlePokemon implements BattleParticipant { id: ID! name: String! displayName: String! currentHP: Int! }",
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor(
            "query { battleParticipant(id: \"1\") { id name ... on BattlePokemon { current*HP } } }",
        );
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

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
        host.add_file(
            &path,
            "type Query {",
            Language::GraphQL,
            DocumentKind::Schema,
        );

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
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { ...UserFields }";
        host.add_file(
            &query_file,
            query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment that references User
        let fragment_file = FilePath::new("file:///fragment.graphql");
        let (fragment_text, cursor_pos) = extract_cursor("fragment F on U*ser { id }");
        host.add_file(
            &fragment_file,
            &fragment_text,
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor("query { u*ser }");
        dbg!(&query_text);
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor("query { user { na*me } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
    fn test_goto_definition_on_schema_field_returns_itself() {
        // When cmd+clicking a schema field definition, return its own location.
        // VSCode will then show "Find References" peek window as fallback.
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) =
            extract_cursor("type User {\n  na*me: String!\n  age: Int!\n}");
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should return field's own location for schema field definition"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to the "name" field on line 1
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_field_in_inline_fragment() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { battleParticipant(id: ID!): BattleParticipant }\ninterface BattleParticipant { id: ID! name: String! displayName: String! }\ntype BattlePokemon implements BattleParticipant { id: ID! name: String! displayName: String! currentHP: Int! }",
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor(
            "query { battleParticipant(id: \"1\") { id name ... on BattlePokemon { current*HP } } }",
        );
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on $id in the argument value
        let (query_text, cursor_pos) =
            extract_cursor("query GetUser($id: ID!) { user(id: $i*d) { name } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on "id" argument name in the query
        let (query_text, cursor_pos) = extract_cursor("query { user(i*d: \"123\") { id } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on the operation name "GetHello"
        let (query_text, cursor_pos) = extract_cursor("query GetH*ello { hello }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
        host.add_file(
            &schema_file,
            schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add queries that use the fragment
        let query1_file = FilePath::new("file:///query1.graphql");
        host.add_file(
            &query1_file,
            "query { ...F }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let query2_file = FilePath::new("file:///query2.graphql");
        host.add_file(
            &query2_file,
            "query { ...F }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { ...F }",
            Language::GraphQL,
            DocumentKind::Executable,
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
        host.add_file(
            &user_file,
            "type User { id: ID }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add types that reference User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "type Query { user: User }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let mutation_file = FilePath::new("file:///mutation.graphql");
        host.add_file(
            &mutation_file,
            "type Mutation { u: User }",
            Language::GraphQL,
            DocumentKind::Schema,
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
        host.add_file(
            &user_file,
            "type User { id: ID }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a type that references User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "type Query { user: User }",
            Language::GraphQL,
            DocumentKind::Schema,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a query that uses the name field
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { id name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a fragment that also uses the name field
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { name }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { profile { bio } } }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL, DocumentKind::Schema,
        );

        // Query that uses the field on the implementing type
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { node { ... on User { id } } }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query with cursor in selection set
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { user { id } }";
        //                                 ^ cursor here at position 15 (right after { before id)
        host.add_file(
            &query_file,
            query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query with cursor OUTSIDE any selection set (at document level)
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { user { id } }\n";
        //                                       ^ cursor at end (position 22 on line 0)
        host.add_file(
            &query_file,
            query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );

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
            Language::GraphQL, DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment BattleDetailed on Battle { id status }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL, DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment BattleDetailed on Battle { id status }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let gql_path = FilePath::new("file:///battle.graphql");
        host.add_file(
            &gql_path,
            &graphql,
            Language::GraphQL,
            DocumentKind::Executable,
        );
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
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

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
            host.add_file(
                &ts_path1,
                &graphql1,
                Language::GraphQL,
                DocumentKind::Executable,
            );
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
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

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
            host.add_file(
                &ts_path2,
                &graphql2,
                Language::GraphQL,
                DocumentKind::Executable,
            );
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
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

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
            host.add_file(
                &path1,
                &graphql1,
                Language::GraphQL,
                DocumentKind::Executable,
            );
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
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

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
            host.add_file(
                &path2,
                &graphql2,
                Language::GraphQL,
                DocumentKind::Executable,
            );
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
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

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
            host.add_file(
                &path3,
                &graphql3,
                Language::GraphQL,
                DocumentKind::Executable,
            );
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
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

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
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
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
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

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
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
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
            Language::GraphQL,
            DocumentKind::Schema,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let path = FilePath::new("file:///queries.graphql");
        host.add_file(
            &path,
            "query GetUser { user }\nmutation CreateUser { createUser }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let path = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &path,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operations
        let queries_path = FilePath::new("file:///queries.graphql");
        host.add_file(
            &queries_path,
            "query GetUser { user { id } }\nquery GetUsers { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
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
        host.add_file(
            &path,
            "type UserProfile { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
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
                extensions: None,
            };

            // Load schemas
            let mut host = AnalysisHost::new();
            host.set_extract_config(graphql_extract::ExtractConfig {
                allow_global_identifiers: false,
                ..Default::default()
            });
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 1 extracted schema from TS
            assert_eq!(
                result.loaded_count, 2,
                "Should load 2 schema files (builtins + extracted)"
            );
            assert!(
                result.pending_introspections.is_empty(),
                "No pending introspections expected"
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
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 2 extracted blocks
            assert_eq!(
                result.loaded_count, 3,
                "Should load 3 schema files (builtins + 2 blocks)"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify both types are available
            let query_symbols = snapshot.workspace_symbols("Query");
            assert!(!query_symbols.is_empty(), "Query type should be found");

            let post_symbols = snapshot.workspace_symbols("Post");
            assert!(!post_symbols.is_empty(), "Post type should be found");
        }

        #[test]
        fn test_multiple_block_uris_use_line_ranges() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript file with multiple GraphQL blocks
            // The blocks start at different lines to verify URI format
            let ts_content = r#"import { gql } from 'graphql-tag';

export const types = gql`
  type Query {
    posts: [Post!]!
  }
`;

export const postType = gql`
  type Post {
    id: ID!
    title: String!
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
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let _ = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            host.rebuild_project_files();

            // Get all files and check their URIs
            let files = host.files();
            let ts_file_uri = path_to_file_uri(&ts_path);

            // Find files from the TS schema
            let block_uris: Vec<_> = files
                .into_iter()
                .map(|f| f.0)
                .filter(|uri| uri.starts_with(&ts_file_uri) && uri.contains('#'))
                .collect();

            // With multiple blocks, URIs should have line-range fragments
            assert_eq!(block_uris.len(), 2, "Should have 2 block URIs");

            // Check that URIs use line-range format (#L{start}-L{end}) not block index (#block0)
            for uri in &block_uris {
                assert!(
                    uri.contains("#L") && uri.contains("-L"),
                    "Block URI should use line-range format (#L{{start}}-L{{end}}), got: {uri}"
                );
                assert!(
                    !uri.contains("#block"),
                    "Block URI should NOT use block index format, got: {uri}"
                );
            }
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
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 1 GraphQL file + 1 TS extraction
            assert_eq!(result.loaded_count, 3, "Should load 3 schema files");

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
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load Apollo client builtins (no GraphQL found in TS file)
            assert_eq!(
                result.loaded_count, 1,
                "Should only load builtins when no GraphQL found"
            );
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
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 Apollo client builtins + 1 extracted schema from JS
            assert_eq!(
                result.loaded_count, 2,
                "Should load 2 schema files (builtins + extracted)"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the Product type is available
            let symbols = snapshot.workspace_symbols("Product");
            assert!(!symbols.is_empty(), "Product type should be found");
        }

        #[test]
        fn test_load_introspection_schema_config() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Config with introspection endpoint
            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Introspection(
                    graphql_config::IntrospectionSchemaConfig {
                        url: "https://api.example.com/graphql".to_string(),
                        headers: Some(
                            [("Authorization".to_string(), "Bearer token".to_string())]
                                .into_iter()
                                .collect(),
                        ),
                        timeout: Some(60),
                        retry: Some(3),
                    },
                ),
                documents: None,
                include: None,
                exclude: None,
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load Apollo client builtins (introspection needs async fetch)
            assert_eq!(
                result.loaded_count, 1,
                "Should load builtins, introspection is async"
            );

            // Should have one pending introspection
            assert_eq!(
                result.pending_introspections.len(),
                1,
                "Should have one pending introspection"
            );

            let pending = &result.pending_introspections[0];
            assert_eq!(pending.url, "https://api.example.com/graphql");
            assert!(pending.headers.is_some());
            assert_eq!(pending.timeout, Some(60));
            assert_eq!(pending.retry, Some(3));

            // Verify virtual_uri generation
            assert_eq!(
                pending.virtual_uri(),
                "schema://api.example.com/graphql/schema.graphql"
            );
        }

        #[test]
        fn test_load_url_schema_pattern() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Config with URL pattern (simpler than full introspection config)
            let config = graphql_config::ProjectConfig {
                schema: graphql_config::SchemaConfig::Path(
                    "https://api.example.com/graphql".to_string(),
                ),
                documents: None,
                include: None,
                exclude: None,
                extensions: None,
            };

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load Apollo client builtins
            assert_eq!(
                result.loaded_count, 1,
                "Should load builtins, URL schema is async"
            );

            // Should have one pending introspection (from URL pattern)
            assert_eq!(
                result.pending_introspections.len(),
                1,
                "Should have one pending introspection from URL"
            );

            let pending = &result.pending_introspections[0];
            assert_eq!(pending.url, "https://api.example.com/graphql");
            assert!(pending.headers.is_none()); // URL patterns don't have headers
        }

        #[test]
        fn test_add_introspected_schema() {
            let mut host = AnalysisHost::new();

            // Simulate adding an introspected schema
            let url = "https://api.example.com/graphql";
            let sdl = r#"
                type Query {
                    user(id: ID!): User
                }

                type User {
                    id: ID!
                    name: String!
                }
            "#;

            let virtual_uri = host.add_introspected_schema(url, sdl);

            // Verify the virtual URI format
            assert_eq!(
                virtual_uri,
                "schema://api.example.com/graphql/schema.graphql"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the types are available
            let user_symbols = snapshot.workspace_symbols("User");
            assert!(!user_symbols.is_empty(), "User type should be found");
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment file with a single fragment
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add fragment file
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Update the same file (simulating did_change)
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add fragment file with one URI format (simulating glob discovery)
        let fragment_file_glob = FilePath::new("file:///home/user/fragments.graphql");
        host.add_file(
            &fragment_file_glob,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            extensions: None,
        };

        let mut host = AnalysisHost::new();
        host.load_schemas_from_config(&config, temp_dir.path())
            .unwrap();

        // Manually add the document file
        let doc_uri = path_to_file_uri(&doc_path);
        let file_path = FilePath::new(&doc_uri);
        host.add_file(
            &file_path,
            doc_content.trim(),
            graphql_base_db::Language::GraphQL,
            DocumentKind::Executable,
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
            "Should have at least one deprecated token, got tokens: {tokens:?}"
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
    fn test_hover_field_in_typescript_file() {
        // Reproduces issue #398: Hover is broken for fields in TypeScript files
        //
        // The bug: find_parent_type_at_offset and walk_type_stack_to_offset were using
        // parse.tree (empty placeholder for TS files) instead of block_context.tree.

        let mut host = AnalysisHost::new();

        // Add a schema file
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            r#"type Query { pokemon(id: ID!): Pokemon }
type Pokemon { id: ID! name: String! }
"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a TypeScript file with embedded GraphQL
        let ts_file = FilePath::new("file:///query.ts");
        let ts_content = r#"import { gql } from '@apollo/client';

export const GET_POKEMON = gql`
  query GetPokemon($id: ID!) {
    pokemon(id: $id) {
      id
      name
    }
  }
`;
"#;
        host.add_file(
            &ts_file,
            ts_content,
            Language::TypeScript,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Hover over the "name" field (line 6, character ~6 in the TS file)
        // Line 6 (0-indexed) is "      name"
        // The "name" field starts at character 6
        let hover = snapshot.hover(&ts_file, Position::new(6, 7));

        // Should return hover info for the field
        assert!(
            hover.is_some(),
            "Hover should work for fields in TypeScript files (issue #398)"
        );
        let hover = hover.unwrap();
        assert!(
            hover.contents.contains("name"),
            "Hover should show field name. Got: {}",
            hover.contents
        );
        assert!(
            hover.contents.contains("String"),
            "Hover should show field type. Got: {}",
            hover.contents
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
            Language::GraphQL,
            DocumentKind::Schema,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
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
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let doc_path2 = FilePath::new("file:///query2.graphql");
        host.add_file(
            &doc_path2,
            r#"query GetUsers {
    users {
        legacyId
    }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a document file
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
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

    #[test]
    fn test_complexity_analysis_basic() {
        let mut host = AnalysisHost::new();

        // Add schema
        let schema = r#"
type Query {
    user(id: ID!): User
    posts: [Post!]!
}

type User {
    id: ID!
    name: String!
    email: String
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
    author: User!
    comments: [Comment!]!
}

type Comment {
    id: ID!
    text: String!
}
"#;
        host.add_file(
            &FilePath::new("file:///schema.graphql"),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operation
        let query = r#"
query GetUser {
    user(id: "123") {
        id
        name
        posts {
            id
            title
        }
    }
}
"#;
        host.add_file(
            &FilePath::new("file:///query.graphql"),
            query,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let results = snapshot.complexity_analysis();

        assert_eq!(results.len(), 1);
        let analysis = &results[0];

        assert_eq!(analysis.operation_name, "GetUser");
        assert_eq!(analysis.operation_type, "query");
        assert!(analysis.total_complexity > 0);
        assert!(analysis.depth > 0);
    }

    #[test]
    fn test_complexity_analysis_list_fields() {
        let mut host = AnalysisHost::new();

        // Add schema
        let schema = r#"
type Query {
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
}
"#;
        host.add_file(
            &FilePath::new("file:///schema.graphql"),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operation with list field
        let query = r#"
query GetPosts {
    posts {
        id
        title
    }
}
"#;
        host.add_file(
            &FilePath::new("file:///query.graphql"),
            query,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let results = snapshot.complexity_analysis();

        assert_eq!(results.len(), 1);
        let analysis = &results[0];

        // List field should have multiplier applied
        assert!(analysis.total_complexity >= 10); // Default list multiplier is 10
    }

    #[test]
    fn test_complexity_analysis_connection_detection() {
        let mut host = AnalysisHost::new();

        // Add schema with Relay connection pattern
        let schema = r#"
type Query {
    users(first: Int): UserConnection!
}

type UserConnection {
    edges: [UserEdge!]!
    pageInfo: PageInfo!
}

type UserEdge {
    node: User!
    cursor: String!
}

type User {
    id: ID!
    name: String!
}

type PageInfo {
    hasNextPage: Boolean!
}
"#;
        host.add_file(
            &FilePath::new("file:///schema.graphql"),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operation with connection pattern
        let query = r#"
query GetUsers {
    users(first: 10) {
        edges {
            node {
                id
                name
            }
        }
    }
}
"#;
        host.add_file(
            &FilePath::new("file:///query.graphql"),
            query,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let results = snapshot.complexity_analysis();

        assert_eq!(results.len(), 1);
        let analysis = &results[0];

        // Should detect connection pattern in breakdown
        let has_connection_field = analysis.breakdown.iter().any(|f| f.is_connection);
        assert!(has_connection_field);
    }

    #[test]
    fn test_add_files_batch() {
        let mut host = AnalysisHost::new();

        // Add multiple files in batch
        let files = vec![
            (
                FilePath::new("file:///schema.graphql"),
                "type Query { user: User } type User { id: ID! name: String! }",
                Language::GraphQL,
                DocumentKind::Schema,
            ),
            (
                FilePath::new("file:///query1.graphql"),
                "query GetUser { user { id name } }",
                Language::GraphQL,
                DocumentKind::Executable,
            ),
            (
                FilePath::new("file:///query2.graphql"),
                "query GetUserName { user { name } }",
                Language::GraphQL,
                DocumentKind::Executable,
            ),
        ];

        host.add_files_batch(&files);

        // Verify all files are accessible
        let snapshot = host.snapshot();

        // Check diagnostics work for all files
        let path1 = FilePath::new("file:///query1.graphql");
        let path2 = FilePath::new("file:///query2.graphql");

        // Both files should be accessible (diagnostics call should not panic)
        let _diag1 = snapshot.diagnostics(&path1);
        let _diag2 = snapshot.diagnostics(&path2);
        // If we got here without panic, files are properly loaded
    }

    #[test]
    fn test_add_files_batch_empty() {
        let mut host = AnalysisHost::new();

        // Add empty batch should not panic
        let files: Vec<(FilePath, &str, Language, DocumentKind)> = vec![];
        host.add_files_batch(&files);

        // Should still be able to get snapshot
        let _snapshot = host.snapshot();
    }

    #[test]
    fn test_add_files_batch_update_existing() {
        let mut host = AnalysisHost::new();

        // First batch
        let files1 = vec![(
            FilePath::new("file:///schema.graphql"),
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        )];
        host.add_files_batch(&files1);

        // Second batch with same file (update) and new file
        let files2 = vec![
            (
                FilePath::new("file:///schema.graphql"),
                "type Query { hello: String world: String }",
                Language::GraphQL,
                DocumentKind::Schema,
            ),
            (
                FilePath::new("file:///query.graphql"),
                "query { hello }",
                Language::GraphQL,
                DocumentKind::Executable,
            ),
        ];
        host.add_files_batch(&files2);

        // Verify updated content
        let snapshot = host.snapshot();
        let schema_path = FilePath::new("file:///schema.graphql");

        // Hover on "world" field should work (proves update happened)
        let hover = snapshot.hover(&schema_path, Position::new(0, 30)); // Position in "world"
        assert!(hover.is_some());
    }

    #[test]
    fn test_batch_loading_is_efficient() {
        let mut host = AnalysisHost::new();

        // Create many files
        let schema = (
            FilePath::new("file:///schema.graphql"),
            "type Query { user: User } type User { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let mut files = vec![schema];
        for i in 0..100 {
            files.push((
                FilePath::new(format!("file:///query{i}.graphql")),
                "query GetUser { user { id name } }",
                Language::GraphQL,
                DocumentKind::Executable,
            ));
        }

        // Convert to borrowed form for add_files_batch
        let files_borrowed: Vec<(FilePath, &str, Language, DocumentKind)> = files
            .iter()
            .map(|(p, c, l, k)| (p.clone(), *c, *l, *k))
            .collect();

        // This should complete quickly (O(n) not O(n²))
        let start = std::time::Instant::now();
        host.add_files_batch(&files_borrowed);
        let elapsed = start.elapsed();

        // Should complete in reasonable time (< 5 seconds even for 100 files)
        assert!(
            elapsed.as_secs() < 5,
            "Batch loading took too long: {elapsed:?}"
        );

        // Verify files are loaded
        let snapshot = host.snapshot();
        let last_file = FilePath::new("file:///query99.graphql");
        // If we can get diagnostics without panic, file is loaded
        let _diag = snapshot.diagnostics(&last_file);
    }

    #[test]
    fn test_unused_fields_lint_with_typescript_file() {
        // Test that fields used in TypeScript embedded GraphQL are correctly tracked
        // and NOT flagged as unused by the unused_fields lint
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema with fields
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            r#"
                type Query { rateLimit: RateLimit }
                type RateLimit {
                    cost: Int!
                    limit: Int!
                    nodeCount: Int!
                }
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a TypeScript file with embedded GraphQL that uses the fields
        let ts_file = FilePath::new("file:///api.ts");
        host.add_file(
            &ts_file,
            r#"
import { gql } from "@apollo/client";

export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      nodeCount
    }
  }
`;
            "#,
            Language::TypeScript,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Check for unused_fields warnings
        let unused_fields_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unused_fields"))
            .collect();

        // nodeCount should NOT be flagged as unused since it's used in the TS file
        let nodecount_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("nodeCount"))
            .collect();

        assert!(
            nodecount_errors.is_empty(),
            "nodeCount is used in TypeScript file and should NOT be flagged as unused. Got: {:?}",
            nodecount_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );

        // All fields (cost, limit, nodeCount) are used, so there should be no unused_fields warnings
        // for RateLimit type fields
        let ratelimit_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("RateLimit"))
            .collect();

        assert!(
            ratelimit_errors.is_empty(),
            "All RateLimit fields are used in TypeScript file. Got: {:?}",
            ratelimit_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_unused_fields_lint_with_config_loaded_typescript() {
        use std::io::Write;

        // Simulate the real LSP scenario: files loaded from config, then lint runs on save
        let temp_dir = tempfile::tempdir().unwrap();

        // Create schema file
        let schema_content = r#"
type Query { rateLimit: RateLimit }
type RateLimit {
    cost: Int!
    limit: Int!
    nodeCount: Int!
}
"#;
        let schema_path = temp_dir.path().join("schema.graphql");
        let mut file = std::fs::File::create(&schema_path).unwrap();
        file.write_all(schema_content.as_bytes()).unwrap();

        // Create TypeScript file with embedded GraphQL
        let ts_content = r#"
import { gql } from "@apollo/client";

export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      nodeCount
    }
  }
`;
"#;
        let ts_path = temp_dir.path().join("api.ts");
        let mut ts_file = std::fs::File::create(&ts_path).unwrap();
        ts_file.write_all(ts_content.as_bytes()).unwrap();

        // Create config that includes both schema and TS documents
        let config = graphql_config::ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern("*.ts".to_string())),
            include: None,
            exclude: None,
            extensions: None,
        };

        // Create host and load files from config (simulating LSP initialization)
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Load schema
        let _ = host.load_schemas_from_config(&config, temp_dir.path());
        // Load documents (including TS files)
        host.load_documents_from_config(&config, temp_dir.path());

        // Rebuild project files to update indices (this happens in LSP initialization)
        host.rebuild_project_files();

        // Get snapshot and run lints (simulating did_save)
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Check for unused_fields warnings
        let unused_fields_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unused_fields"))
            .collect();

        // nodeCount should NOT be flagged as unused
        let nodecount_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("nodeCount"))
            .collect();

        assert!(
            nodecount_errors.is_empty(),
            "nodeCount used in config-loaded TypeScript file should NOT be flagged as unused. Got: {:?}",
            nodecount_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_unused_fields_lint_simulating_save_after_open() {
        use std::io::Write;

        // This test simulates the exact user scenario:
        // 1. Config is loaded (schema + documents)
        // 2. Schema file is opened (via goto_definition or directly)
        // 3. Schema file is saved (triggers project-wide lint)
        //
        // The lint should correctly see all document files including TS files.

        let temp_dir = tempfile::tempdir().unwrap();

        // Create schema file
        let schema_content = r#"
type Query { rateLimit: RateLimit }
type RateLimit {
    cost: Int!
    limit: Int!
    nodeCount: Int!
}
"#;
        let schema_path = temp_dir.path().join("schema.graphql");
        let mut file = std::fs::File::create(&schema_path).unwrap();
        file.write_all(schema_content.as_bytes()).unwrap();

        // Create TypeScript file with embedded GraphQL
        let ts_content = r#"
import { gql } from "@apollo/client";

export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      nodeCount
    }
  }
`;
"#;
        let ts_path = temp_dir.path().join("api.ts");
        let mut ts_file = std::fs::File::create(&ts_path).unwrap();
        ts_file.write_all(ts_content.as_bytes()).unwrap();

        // Create config
        let config = graphql_config::ProjectConfig {
            schema: graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            documents: Some(graphql_config::DocumentsConfig::Pattern("*.ts".to_string())),
            include: None,
            exclude: None,
            extensions: None,
        };

        // Create host and load files from config (simulating LSP initialization)
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Load schema
        let _ = host.load_schemas_from_config(&config, temp_dir.path());
        // Load documents (including TS files)
        let loaded_docs = host.load_documents_from_config(&config, temp_dir.path());

        // Verify the TS file was loaded
        assert!(
            loaded_docs
                .iter()
                .any(|f| f.path.as_str().ends_with("api.ts")),
            "api.ts should be loaded by load_documents_from_config. Loaded: {:?}",
            loaded_docs
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>()
        );

        // Rebuild is called by add_files_batch internally, but let's also call it explicitly
        // to ensure we're in a consistent state
        host.rebuild_project_files();

        // Now simulate did_open for the schema file (as if user navigated to it)
        // Use path_to_file_uri for consistent path handling across platforms
        let schema_file_path = FilePath::new(crate::helpers::path_to_file_uri(&schema_path));
        let (is_new, snapshot) = host.update_file_and_snapshot(
            &schema_file_path,
            schema_content,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Schema should NOT be new (already loaded from config)
        assert!(
            !is_new,
            "Schema file should already exist (loaded from config)"
        );

        // Now run project-wide lints (simulating did_save)
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Debug: print all diagnostics
        for (path, diags) in &project_diagnostics {
            for d in diags {
                eprintln!(
                    "Diagnostic in {}: {} ({})",
                    path.as_str(),
                    d.message,
                    d.code.as_deref().unwrap_or("")
                );
            }
        }

        // Check for unused_fields warnings
        let unused_fields_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unused_fields"))
            .collect();

        // nodeCount should NOT be flagged as unused
        let nodecount_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("nodeCount"))
            .collect();

        assert!(
            nodecount_errors.is_empty(),
            "nodeCount used in TypeScript file should NOT be flagged as unused after save. Got: {:?}",
            nodecount_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_all_diagnostics_for_file_merges_per_file_and_project_wide() {
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            r#"
                type Query { user: User }
                type User {
                    id: ID!
                    unusedField: String
                }
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_file,
            "query { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();
        let snapshot = host.snapshot();

        // all_diagnostics_for_file should include project-wide diagnostics
        let schema_diags = snapshot.all_diagnostics_for_file(&schema_file);
        let has_unused_field = schema_diags.iter().any(|d| {
            d.code.as_deref() == Some("unused_fields") && d.message.contains("unusedField")
        });

        assert!(
            has_unused_field,
            "all_diagnostics_for_file should include project-wide unused_fields diagnostic"
        );
    }

    // ===========================================
    // Inlay Hints Tests
    // ===========================================

    #[test]
    fn test_inlay_hints_for_scalar_fields() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! level: Int! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    name\n    level\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for scalar fields: name and level
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for scalar fields, got {}",
            hints.len()
        );

        // Check that hints contain type information
        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();
        assert!(
            hint_labels.iter().any(|l| l.contains("String")),
            "Expected hint containing String type"
        );
        assert!(
            hint_labels.iter().any(|l| l.contains("Int")),
            "Expected hint containing Int type"
        );
    }

    #[test]
    fn test_inlay_hints_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        let path = FilePath::new("file:///nonexistent.graphql");
        let hints = snapshot.inlay_hints(&path, None);

        assert!(hints.is_empty(), "Expected no hints for nonexistent file");
    }

    #[test]
    fn test_inlay_hints_with_range_filter() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! level: Int! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    name\n    level\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Request hints only for line 2 (where "name" is)
        let range = Some(Range::new(Position::new(2, 0), Position::new(2, 100)));
        let hints = snapshot.inlay_hints(&doc_path, range);

        // Should only get the hint for the field on line 2
        assert!(
            hints.len() == 1,
            "Expected 1 hint for filtered range, got {}",
            hints.len()
        );
        assert!(
            hints[0].label.contains("String"),
            "Expected String type hint for name field"
        );
    }

    #[test]
    fn test_inlay_hints_no_project() {
        let mut host = AnalysisHost::new();

        // Add a file but don't rebuild project (so there's no schema context)
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser { user { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        // Don't call rebuild_project_files()

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Without schema context, no type hints can be generated
        assert!(
            hints.is_empty(),
            "Expected no hints without project context"
        );
    }

    #[test]
    fn test_inlay_hints_nested_fields() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query { user: User }
type User {
  name: String!
  posts: [Post!]!
}
type Post {
  title: String!
  content: String
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"query GetUserWithPosts {
  user {
    name
    posts {
      title
      content
    }
  }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for scalar fields: name, title, content
        assert!(
            hints.len() >= 3,
            "Expected at least 3 inlay hints for nested scalar fields, got {}",
            hints.len()
        );

        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();

        // Check for String type hints
        let string_hints = hint_labels.iter().filter(|l| l.contains("String")).count();
        assert!(
            string_hints >= 2,
            "Expected at least 2 String type hints, got {string_hints}"
        );
    }

    #[test]
    fn test_inlay_hints_fragment_definition() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! age: Int! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "fragment UserFields on User {\n  name\n  age\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for fields in fragment
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for fragment fields, got {}",
            hints.len()
        );

        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();
        assert!(
            hint_labels.iter().any(|l| l.contains("String")),
            "Expected String type hint"
        );
        assert!(
            hint_labels.iter().any(|l| l.contains("Int")),
            "Expected Int type hint"
        );
    }

    #[test]
    fn test_inlay_hints_with_aliases() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! email: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    userName: name\n    userEmail: email\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for aliased scalar fields
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for aliased fields, got {}",
            hints.len()
        );

        // Both hints should show String type
        let string_hints = hints.iter().filter(|h| h.label.contains("String")).count();
        assert_eq!(
            string_hints, 2,
            "Expected 2 String type hints for aliased fields"
        );
    }

    #[test]
    fn test_inlay_hints_typename() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    __typename\n    name\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for both __typename and name
        assert_eq!(
            hints.len(),
            2,
            "Expected 2 inlay hints (for __typename and name), got {}",
            hints.len()
        );

        // Check __typename shows String! hint
        let typename_hint = hints.iter().find(|h| h.label == ": String!");
        assert!(
            typename_hint.is_some(),
            "Expected __typename hint with 'String!' type"
        );
    }
}
