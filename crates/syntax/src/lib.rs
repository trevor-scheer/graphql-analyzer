//! # GraphQL Syntax Parsing
//!
//! This crate provides unified parsing for GraphQL documents, whether from pure
//! `.graphql` files or embedded in TypeScript/JavaScript.
//!
//! ## Unified Document Model
//!
//! All GraphQL content is represented as "blocks" - discrete GraphQL documents
//! with position information. Pure GraphQL files have a single block at offset 0,
//! while TS/JS files may have multiple blocks extracted from template literals.
//!
//! ```rust,ignore
//! // Always use documents() for iteration - works for all file types
//! for doc in parse.documents() {
//!     process(doc.tree, doc.ast, doc.line_offset);
//! }
//! ```
//!
//! ## Block Context
//!
//! Each `DocumentRef` provides:
//! - `tree`: CST for position/token information
//! - `ast`: AST for semantic analysis
//! - `line_offset`: Line number in original file (0 for pure GraphQL)
//! - `source`: The GraphQL source text

use graphql_base_db::{DocumentKind, FileContent, FileMetadata, Language};
use std::sync::Arc;

/// A parse error with position information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Error message
    pub message: String,
    /// Byte offset where the error occurred
    pub offset: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at offset {})", self.message, self.offset)
    }
}

/// Result of parsing a file
///
/// All GraphQL content is represented uniformly as blocks. Pure GraphQL files
/// have a single block at offset 0. Use `documents()` to iterate over blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parse {
    /// GraphQL blocks (always at least one for valid files)
    blocks: Vec<ExtractedBlock>,
    /// Parse errors (syntax errors only, not validation)
    errors: Vec<ParseError>,
}

/// A GraphQL block extracted from a TypeScript/JavaScript file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedBlock {
    /// The extracted GraphQL source code
    pub source: Arc<str>,
    /// The parsed syntax tree for this block (CST for position information)
    pub tree: Arc<apollo_parser::SyntaxTree>,
    /// The parsed AST for this block (AST for semantic analysis)
    pub ast: Arc<apollo_compiler::ast::Document>,
    /// Byte offset in the original file
    pub offset: usize,
    /// Line number in the original file (0-based)
    pub line: u32,
    /// Character offset in the line (0-based, UTF-16 code units)
    pub character: u32,
}

/// A reference to a GraphQL document within a parsed file.
///
/// This provides a unified interface for accessing GraphQL documents,
/// whether from a pure `.graphql` file or extracted from TypeScript/JavaScript.
/// All documents have source text and position information.
#[derive(Debug, Clone, Copy)]
pub struct DocumentRef<'a> {
    /// The syntax tree for this document (CST for position/token information)
    pub tree: &'a apollo_parser::SyntaxTree,
    /// The AST for this document (for semantic analysis)
    pub ast: &'a apollo_compiler::ast::Document,
    /// Line offset in the original file (0 for pure GraphQL files)
    pub line_offset: u32,
    /// Column offset in the original file (0 for pure GraphQL files)
    pub column_offset: u32,
    /// Byte offset in the original file (0 for pure GraphQL files)
    pub byte_offset: usize,
    /// The GraphQL source text
    pub source: &'a str,
}

impl Parse {
    /// Returns an iterator over all GraphQL documents in this file.
    ///
    /// All files yield at least one document. Pure GraphQL files have a single
    /// document at offset 0, while TS/JS files may have multiple documents.
    ///
    /// # Example
    /// ```ignore
    /// for doc in parse.documents() {
    ///     validate_document(doc.tree, doc.ast, doc.line_offset);
    /// }
    /// ```
    pub fn documents(&self) -> impl Iterator<Item = DocumentRef<'_>> {
        self.blocks.iter().map(|block| DocumentRef {
            tree: &block.tree,
            ast: &block.ast,
            line_offset: block.line,
            column_offset: block.character,
            byte_offset: block.offset,
            source: &block.source,
        })
    }

    /// Returns the number of GraphQL documents in this file.
    #[must_use]
    pub fn document_count(&self) -> usize {
        self.blocks.len()
    }

    /// Returns true if there are no GraphQL documents (e.g., TS file with no gql tags).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Returns the parse errors.
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Returns true if there were any parse errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Parse a file into a syntax tree
/// This is the foundation - all semantic analysis builds on this
#[salsa::tracked]
pub fn parse(
    db: &dyn GraphQLSyntaxDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Parse {
    let uri = metadata.uri(db);
    let language = metadata.language(db);

    // Dispatch based on language: GraphQL parses directly, others need extraction
    if language.requires_extraction() {
        extract_and_parse(db, &content.text(db), uri.as_str())
    } else {
        parse_graphql(&content.text(db), uri.as_str())
    }
}

/// Parse pure GraphQL content into a single block at offset 0
fn parse_graphql(content: &str, uri: &str) -> Parse {
    let parser = apollo_parser::Parser::new(content);
    let tree = parser.parse();

    let mut errors: Vec<ParseError> = tree
        .errors()
        .map(|e| ParseError {
            message: e.message().to_string(),
            offset: e.index(),
        })
        .collect();

    let ast = match apollo_compiler::ast::Document::parse(content, uri) {
        Ok(doc) => doc,
        Err(with_errors) => {
            // apollo-compiler errors don't have precise positions, so we use offset 0
            errors.extend(with_errors.errors.iter().map(|e| ParseError {
                message: e.to_string(),
                offset: 0,
            }));
            with_errors.partial
        }
    };

    // Create a single block representing the entire file at offset 0
    let block = ExtractedBlock {
        source: Arc::from(content),
        tree: Arc::new(tree),
        ast: Arc::new(ast),
        offset: 0,
        line: 0,
        character: 0,
    };

    Parse {
        blocks: vec![block],
        errors,
    }
}

/// Extract GraphQL from TypeScript/JavaScript and parse each block
fn extract_and_parse(db: &dyn GraphQLSyntaxDatabase, content: &str, uri: &str) -> Parse {
    use graphql_extract::{extract_from_source, ExtractConfig, Language};

    tracing::debug!(content_len = content.len(), "extract_and_parse called");

    let config = db
        .extract_config()
        .map_or_else(ExtractConfig::default, |arc| (*arc).clone());

    tracing::debug!(
        allow_global_identifiers = config.allow_global_identifiers,
        tag_identifiers = ?config.tag_identifiers,
        "Using extract config"
    );

    let language = Language::TypeScript;
    let extracted = match extract_from_source(content, language, &config) {
        Ok(blocks) => {
            tracing::debug!(blocks_extracted = blocks.len(), "Extraction successful");
            blocks
        }
        Err(e) => {
            tracing::error!(error = ?e, "Extraction failed");
            Vec::new()
        }
    };

    let mut blocks = Vec::new();
    let mut all_errors = Vec::new();

    for block in extracted {
        let parser = apollo_parser::Parser::new(&block.source);
        let tree = parser.parse();

        let block_offset = block.location.offset;
        all_errors.extend(tree.errors().map(|e| ParseError {
            message: e.message().to_string(),
            offset: block_offset + e.index(),
        }));

        let ast = match apollo_compiler::ast::Document::parse(&block.source, uri) {
            Ok(doc) => doc,
            Err(with_errors) => {
                all_errors.extend(with_errors.errors.iter().map(|e| ParseError {
                    message: e.to_string(),
                    offset: block_offset,
                }));
                with_errors.partial
            }
        };

        blocks.push(ExtractedBlock {
            source: Arc::from(block.source.as_str()),
            tree: Arc::new(tree),
            ast: Arc::new(ast),
            offset: block.location.offset,
            line: block.location.range.start.line,
            character: block.location.range.start.character,
        });
    }

    Parse {
        blocks,
        errors: all_errors,
    }
}

/// Line index for a file (for position conversions)
/// Maps byte offsets to line/column positions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    /// Byte offset of the start of each line
    line_starts: Vec<usize>,
}

/// Check if GraphQL content contains schema definitions
///
/// Returns true if the content contains any schema type definitions, extensions,
/// or directive definitions. Used to distinguish schema files from executable documents.
#[must_use]
pub fn content_has_schema_definitions(content: &str) -> bool {
    use apollo_compiler::parser::Parser;

    let mut parser = Parser::new();
    let ast = parser
        .parse_ast(content, "virtual.graphql")
        .unwrap_or_else(|e| e.partial);

    ast.definitions.iter().any(|def| {
        matches!(
            def,
            apollo_compiler::ast::Definition::SchemaDefinition(_)
                | apollo_compiler::ast::Definition::SchemaExtension(_)
                | apollo_compiler::ast::Definition::ObjectTypeDefinition(_)
                | apollo_compiler::ast::Definition::ObjectTypeExtension(_)
                | apollo_compiler::ast::Definition::InterfaceTypeDefinition(_)
                | apollo_compiler::ast::Definition::InterfaceTypeExtension(_)
                | apollo_compiler::ast::Definition::UnionTypeDefinition(_)
                | apollo_compiler::ast::Definition::UnionTypeExtension(_)
                | apollo_compiler::ast::Definition::ScalarTypeDefinition(_)
                | apollo_compiler::ast::Definition::ScalarTypeExtension(_)
                | apollo_compiler::ast::Definition::EnumTypeDefinition(_)
                | apollo_compiler::ast::Definition::EnumTypeExtension(_)
                | apollo_compiler::ast::Definition::InputObjectTypeDefinition(_)
                | apollo_compiler::ast::Definition::InputObjectTypeExtension(_)
                | apollo_compiler::ast::Definition::DirectiveDefinition(_)
        )
    })
}

/// Check if GraphQL content contains executable definitions (operations or fragments).
///
/// Returns true if the content contains any operation definitions (query, mutation,
/// subscription) or fragment definitions.
#[must_use]
pub fn content_has_executable_definitions(content: &str) -> bool {
    use apollo_compiler::parser::Parser;

    let mut parser = Parser::new();
    let ast = parser
        .parse_ast(content, "virtual.graphql")
        .unwrap_or_else(|e| e.partial);

    ast.definitions.iter().any(|def| {
        matches!(
            def,
            apollo_compiler::ast::Definition::OperationDefinition(_)
                | apollo_compiler::ast::Definition::FragmentDefinition(_)
        )
    })
}

/// Describes a mismatch between a file's expected `DocumentKind` (from config)
/// and what was actually found in the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentMismatch {
    /// Expected schema definitions, found executable definitions
    ExpectedSchemaFoundExecutable {
        /// Names of the executable definitions found
        definitions: Vec<String>,
    },
    /// Expected executable definitions, found schema definitions
    ExpectedExecutableFoundSchema {
        /// Names of the schema definitions found
        definitions: Vec<String>,
    },
}

impl ContentMismatch {
    /// Returns a human-readable message describing the mismatch.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::ExpectedSchemaFoundExecutable { definitions } => {
                if definitions.is_empty() {
                    "File in schema config contains executable definitions (operations or fragments)".to_string()
                } else {
                    format!(
                        "File in schema config contains executable definitions: {}",
                        definitions.join(", ")
                    )
                }
            }
            Self::ExpectedExecutableFoundSchema { definitions } => {
                if definitions.is_empty() {
                    "File in documents config contains schema definitions".to_string()
                } else {
                    format!(
                        "File in documents config contains schema definitions: {}",
                        definitions.join(", ")
                    )
                }
            }
        }
    }
}

/// Validate that GraphQL content matches the expected `DocumentKind`.
///
/// Returns `None` if the content is consistent with the expected kind,
/// or `Some(ContentMismatch)` if there's a conflict.
///
/// # Arguments
///
/// * `content` - The GraphQL source content to validate
/// * `expected` - The expected `DocumentKind` from the config
///
/// # Rules
///
/// - Schema files should NOT contain operations or fragments
/// - Executable files should NOT contain type definitions
/// - Empty files or files with only comments are valid for any kind
#[must_use]
pub fn validate_content_matches_kind(
    content: &str,
    expected: DocumentKind,
) -> Option<ContentMismatch> {
    use apollo_compiler::parser::Parser;

    let mut parser = Parser::new();
    let ast = parser
        .parse_ast(content, "virtual.graphql")
        .unwrap_or_else(|e| e.partial);

    match expected {
        DocumentKind::Schema => {
            // Check for executable definitions (operations, fragments)
            let executable_defs: Vec<String> = ast
                .definitions
                .iter()
                .filter_map(|def| match def {
                    apollo_compiler::ast::Definition::OperationDefinition(op) => {
                        Some(op.name.as_ref().map_or_else(
                            || format!("anonymous {}", op.operation_type),
                            ToString::to_string,
                        ))
                    }
                    apollo_compiler::ast::Definition::FragmentDefinition(frag) => {
                        Some(format!("fragment {}", frag.name))
                    }
                    _ => None,
                })
                .collect();

            if executable_defs.is_empty() {
                None
            } else {
                Some(ContentMismatch::ExpectedSchemaFoundExecutable {
                    definitions: executable_defs,
                })
            }
        }
        DocumentKind::Executable => {
            // Check for schema definitions
            let schema_defs: Vec<String> = ast
                .definitions
                .iter()
                .filter_map(|def| match def {
                    apollo_compiler::ast::Definition::SchemaDefinition(_) => {
                        Some("schema".to_string())
                    }
                    apollo_compiler::ast::Definition::SchemaExtension(_) => {
                        Some("extend schema".to_string())
                    }
                    apollo_compiler::ast::Definition::ObjectTypeDefinition(t) => {
                        Some(format!("type {}", t.name))
                    }
                    apollo_compiler::ast::Definition::ObjectTypeExtension(t) => {
                        Some(format!("extend type {}", t.name))
                    }
                    apollo_compiler::ast::Definition::InterfaceTypeDefinition(t) => {
                        Some(format!("interface {}", t.name))
                    }
                    apollo_compiler::ast::Definition::InterfaceTypeExtension(t) => {
                        Some(format!("extend interface {}", t.name))
                    }
                    apollo_compiler::ast::Definition::UnionTypeDefinition(t) => {
                        Some(format!("union {}", t.name))
                    }
                    apollo_compiler::ast::Definition::UnionTypeExtension(t) => {
                        Some(format!("extend union {}", t.name))
                    }
                    apollo_compiler::ast::Definition::ScalarTypeDefinition(t) => {
                        Some(format!("scalar {}", t.name))
                    }
                    apollo_compiler::ast::Definition::ScalarTypeExtension(t) => {
                        Some(format!("extend scalar {}", t.name))
                    }
                    apollo_compiler::ast::Definition::EnumTypeDefinition(t) => {
                        Some(format!("enum {}", t.name))
                    }
                    apollo_compiler::ast::Definition::EnumTypeExtension(t) => {
                        Some(format!("extend enum {}", t.name))
                    }
                    apollo_compiler::ast::Definition::InputObjectTypeDefinition(t) => {
                        Some(format!("input {}", t.name))
                    }
                    apollo_compiler::ast::Definition::InputObjectTypeExtension(t) => {
                        Some(format!("extend input {}", t.name))
                    }
                    apollo_compiler::ast::Definition::DirectiveDefinition(d) => {
                        Some(format!("directive @{}", d.name))
                    }
                    _ => None,
                })
                .collect();

            if schema_defs.is_empty() {
                None
            } else {
                Some(ContentMismatch::ExpectedExecutableFoundSchema {
                    definitions: schema_defs,
                })
            }
        }
    }
}

/// Check if a path has a given extension (case-insensitive)
fn has_extension(path: &str, ext: &str) -> bool {
    path.len() > ext.len()
        && path.as_bytes()[path.len() - ext.len()..].eq_ignore_ascii_case(ext.as_bytes())
}

/// Determine `Language` and `DocumentKind` for files opened/changed in the editor.
///
/// For TypeScript/JavaScript files, determines Language from extension and defaults
/// to `DocumentKind::Executable` (config can override this if schema patterns match).
///
/// For .graphql/.gql files, inspects the content to determine if it contains schema
/// definitions or executable documents.
///
/// This is used as a fallback when no config is available or when a file is opened
/// that doesn't match any configured patterns.
#[must_use]
pub fn determine_file_kind_from_content(path: &str, content: &str) -> (Language, DocumentKind) {
    // Determine language from extension
    let language = if has_extension(path, ".ts") || has_extension(path, ".tsx") {
        Language::TypeScript
    } else if has_extension(path, ".js") || has_extension(path, ".jsx") {
        Language::JavaScript
    } else {
        Language::GraphQL
    };

    // For TS/JS files, default to Executable (operations/fragments)
    // For GraphQL files, inspect content to determine kind
    let document_kind = if language.requires_extraction() {
        DocumentKind::Executable
    } else if content_has_schema_definitions(content) {
        DocumentKind::Schema
    } else {
        DocumentKind::Executable
    };

    (language, document_kind)
}

impl LineIndex {
    /// Create a new line index from source text
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];

        for (i, c) in text.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }

        Self { line_starts }
    }

    /// Convert a byte offset to a line/column position (0-based)
    #[must_use]
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let line = self
            .line_starts
            .binary_search(&offset)
            .unwrap_or_else(|i| i.saturating_sub(1));

        let col = offset - self.line_starts[line];
        (line, col)
    }

    /// Get the byte offset of the start of a line
    #[must_use]
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line).copied()
    }

    /// Get the number of lines
    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

/// Compute line index for a file (for position conversions)
#[salsa::tracked]
pub fn line_index(db: &dyn GraphQLSyntaxDatabase, content: FileContent) -> Arc<LineIndex> {
    Arc::new(LineIndex::new(&content.text(db)))
}

/// The salsa database trait for syntax queries
#[salsa::db]
pub trait GraphQLSyntaxDatabase: salsa::Database {
    /// Get the extract configuration for TypeScript/JavaScript extraction
    /// Returns None by default, which means use `ExtractConfig::default()`
    /// Implementations can override to provide custom configuration
    fn extract_config(&self) -> Option<Arc<graphql_extract::ExtractConfig>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_index_new() {
        let text = "line 1\nline 2\nline 3";
        let index = LineIndex::new(text);

        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_start(0), Some(0));
        assert_eq!(index.line_start(1), Some(7));
        assert_eq!(index.line_start(2), Some(14));
    }

    #[test]
    fn test_line_index_line_col() {
        let text = "line 1\nline 2\nline 3";
        let index = LineIndex::new(text);

        assert_eq!(index.line_col(0), (0, 0));
        assert_eq!(index.line_col(5), (0, 5));
        assert_eq!(index.line_col(7), (1, 0));
        assert_eq!(index.line_col(10), (1, 3));
        assert_eq!(index.line_col(14), (2, 0));
    }

    #[test]
    fn test_parse_graphql() {
        let content = "type User { id: ID! }";
        let parse = parse_graphql(content, "test.graphql");

        assert!(!parse.has_errors());
        assert_eq!(parse.document_count(), 1);

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs[0].tree.document().definitions().count(), 1);
        assert_eq!(docs[0].line_offset, 0);
        assert_eq!(docs[0].source, content);
    }

    #[test]
    fn test_parse_graphql_with_error() {
        let content = "type User {";
        let parse = parse_graphql(content, "test.graphql");

        assert!(parse.has_errors());
    }

    #[test]
    fn test_line_index_empty() {
        let index = LineIndex::new("");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_col(0), (0, 0));
    }

    #[test]
    fn test_line_index_single_line() {
        let index = LineIndex::new("hello");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_col(0), (0, 0));
        assert_eq!(index.line_col(3), (0, 3));
    }

    #[test]
    fn test_content_has_schema_definitions_true() {
        let schema_content = "type User { id: ID! }";
        assert!(content_has_schema_definitions(schema_content));

        let interface_content = "interface Node { id: ID! }";
        assert!(content_has_schema_definitions(interface_content));

        let enum_content = "enum Status { ACTIVE INACTIVE }";
        assert!(content_has_schema_definitions(enum_content));
    }

    #[test]
    fn test_content_has_schema_definitions_false() {
        let query_content = "query GetUser { user { id } }";
        assert!(!content_has_schema_definitions(query_content));

        let fragment_content = "fragment UserFields on User { id name }";
        assert!(!content_has_schema_definitions(fragment_content));

        let mutation_content = "mutation UpdateUser { updateUser { id } }";
        assert!(!content_has_schema_definitions(mutation_content));
    }

    #[test]
    fn test_content_has_schema_definitions_mixed() {
        let mixed_content = "type User { id: ID! }\nquery GetUser { user { id } }";
        assert!(content_has_schema_definitions(mixed_content));
    }

    #[test]
    fn test_content_has_executable_definitions_true() {
        let query_content = "query GetUser { user { id } }";
        assert!(content_has_executable_definitions(query_content));

        let fragment_content = "fragment UserFields on User { id name }";
        assert!(content_has_executable_definitions(fragment_content));

        let mutation_content = "mutation UpdateUser { updateUser { id } }";
        assert!(content_has_executable_definitions(mutation_content));

        let subscription_content = "subscription OnUserChange { userChanged { id } }";
        assert!(content_has_executable_definitions(subscription_content));
    }

    #[test]
    fn test_content_has_executable_definitions_false() {
        let schema_content = "type User { id: ID! }";
        assert!(!content_has_executable_definitions(schema_content));

        let interface_content = "interface Node { id: ID! }";
        assert!(!content_has_executable_definitions(interface_content));

        let enum_content = "enum Status { ACTIVE INACTIVE }";
        assert!(!content_has_executable_definitions(enum_content));
    }

    #[test]
    fn test_content_has_executable_definitions_mixed() {
        // Mixed files have both schema and executable definitions
        let mixed_content = "type User { id: ID! }\nquery GetUser { user { id } }";
        assert!(content_has_executable_definitions(mixed_content));
    }

    #[test]
    fn test_validate_content_matches_kind_schema_valid() {
        // Schema file with only schema definitions - valid
        let content = "type User { id: ID! }\ninterface Node { id: ID! }";
        assert!(validate_content_matches_kind(content, DocumentKind::Schema).is_none());
    }

    #[test]
    fn test_validate_content_matches_kind_schema_invalid() {
        // Schema file with executable definitions - invalid
        let content = "type User { id: ID! }\nquery GetUser { user { id } }";
        let mismatch = validate_content_matches_kind(content, DocumentKind::Schema);
        assert!(mismatch.is_some());

        let mismatch = mismatch.unwrap();
        match &mismatch {
            ContentMismatch::ExpectedSchemaFoundExecutable { definitions } => {
                assert!(definitions.iter().any(|d| d.contains("GetUser")));
            }
            ContentMismatch::ExpectedExecutableFoundSchema { .. } => {
                panic!("Expected ExpectedSchemaFoundExecutable")
            }
        }
        // Check message generation
        assert!(mismatch.message().contains("GetUser"));
    }

    #[test]
    fn test_validate_content_matches_kind_executable_valid() {
        // Executable file with only operations and fragments - valid
        let content = "query GetUser { user { id } }\nfragment UserFields on User { id }";
        assert!(validate_content_matches_kind(content, DocumentKind::Executable).is_none());
    }

    #[test]
    fn test_validate_content_matches_kind_executable_invalid() {
        // Executable file with schema definitions - invalid
        let content = "query GetUser { user { id } }\ntype User { id: ID! }";
        let mismatch = validate_content_matches_kind(content, DocumentKind::Executable);
        assert!(mismatch.is_some());

        let mismatch = mismatch.unwrap();
        match &mismatch {
            ContentMismatch::ExpectedExecutableFoundSchema { definitions } => {
                assert!(definitions.iter().any(|d| d.contains("User")));
            }
            ContentMismatch::ExpectedSchemaFoundExecutable { .. } => {
                panic!("Expected ExpectedExecutableFoundSchema")
            }
        }
        // Check message generation
        assert!(mismatch.message().contains("type User"));
    }

    #[test]
    fn test_validate_content_matches_kind_empty() {
        // Empty content is valid for any kind
        assert!(validate_content_matches_kind("", DocumentKind::Schema).is_none());
        assert!(validate_content_matches_kind("", DocumentKind::Executable).is_none());
    }

    #[test]
    fn test_validate_content_matches_kind_only_comments() {
        // Content with only comments is valid for any kind
        let content = "# This is a comment\n# Another comment";
        assert!(validate_content_matches_kind(content, DocumentKind::Schema).is_none());
        assert!(validate_content_matches_kind(content, DocumentKind::Executable).is_none());
    }

    #[test]
    fn test_validate_content_anonymous_operation() {
        // Anonymous operations should be detected
        let content = "{ user { id } }";
        let mismatch = validate_content_matches_kind(content, DocumentKind::Schema);
        assert!(mismatch.is_some());

        match mismatch.unwrap() {
            ContentMismatch::ExpectedSchemaFoundExecutable { definitions } => {
                assert!(definitions.iter().any(|d| d.contains("anonymous")));
            }
            ContentMismatch::ExpectedExecutableFoundSchema { .. } => {
                panic!("Expected ExpectedSchemaFoundExecutable")
            }
        }
    }

    #[test]
    fn test_determine_file_kind_typescript() {
        let content = "const query = gql`query { user { id } }`;";
        assert_eq!(
            determine_file_kind_from_content("file.ts", content),
            (Language::TypeScript, DocumentKind::Executable)
        );
        assert_eq!(
            determine_file_kind_from_content("file.tsx", content),
            (Language::TypeScript, DocumentKind::Executable)
        );
    }

    #[test]
    fn test_determine_file_kind_javascript() {
        let content = "const query = gql`query { user { id } }`;";
        assert_eq!(
            determine_file_kind_from_content("file.js", content),
            (Language::JavaScript, DocumentKind::Executable)
        );
        assert_eq!(
            determine_file_kind_from_content("file.jsx", content),
            (Language::JavaScript, DocumentKind::Executable)
        );
    }

    #[test]
    fn test_determine_file_kind_schema() {
        let content = "type User { id: ID! }";
        assert_eq!(
            determine_file_kind_from_content("schema.graphql", content),
            (Language::GraphQL, DocumentKind::Schema)
        );
    }

    #[test]
    fn test_determine_file_kind_executable() {
        let content = "query GetUser { user { id } }";
        assert_eq!(
            determine_file_kind_from_content("query.graphql", content),
            (Language::GraphQL, DocumentKind::Executable)
        );
    }

    #[test]
    fn test_apollo_parser_error_info() {
        let content = "type User {"; // Invalid GraphQL
        let parser = apollo_parser::Parser::new(content);
        let tree = parser.parse();
        assert!(tree.errors().next().is_some());
    }

    #[test]
    fn test_documents_iterator_pure_graphql() {
        let content = "type User { id: ID! }\ntype Post { id: ID! }";
        let parse = parse_graphql(content, "test.graphql");

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs.len(), 1);

        let doc = &docs[0];
        assert_eq!(doc.line_offset, 0);
        assert_eq!(doc.column_offset, 0);
        assert_eq!(doc.source, content);
        assert_eq!(doc.tree.document().definitions().count(), 2);
    }

    #[test]
    fn test_documents_iterator_with_blocks() {
        let parse = Parse {
            blocks: vec![
                ExtractedBlock {
                    source: Arc::from("query Q1 { user { id } }"),
                    tree: Arc::new(apollo_parser::Parser::new("query Q1 { user { id } }").parse()),
                    ast: Arc::new(
                        apollo_compiler::ast::Document::parse("query Q1 { user { id } }", "test")
                            .unwrap(),
                    ),
                    offset: 100,
                    line: 5,
                    character: 10,
                },
                ExtractedBlock {
                    source: Arc::from("query Q2 { post { id } }"),
                    tree: Arc::new(apollo_parser::Parser::new("query Q2 { post { id } }").parse()),
                    ast: Arc::new(
                        apollo_compiler::ast::Document::parse("query Q2 { post { id } }", "test")
                            .unwrap(),
                    ),
                    offset: 200,
                    line: 10,
                    character: 15,
                },
            ],
            errors: vec![],
        };

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs.len(), 2);

        assert_eq!(docs[0].line_offset, 5);
        assert_eq!(docs[0].column_offset, 10);
        assert_eq!(docs[0].source, "query Q1 { user { id } }");

        assert_eq!(docs[1].line_offset, 10);
        assert_eq!(docs[1].column_offset, 15);
        assert_eq!(docs[1].source, "query Q2 { post { id } }");
    }

    #[test]
    fn test_documents_iterator_single_block() {
        let content = "type User { id: ID! }";
        let parse = parse_graphql(content, "test.graphql");

        assert_eq!(parse.document_count(), 1);
        assert!(!parse.is_empty());

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].line_offset, 0);
        assert_eq!(docs[0].source, content);
    }

    #[test]
    fn test_empty_ts_file_no_graphql() {
        // Simulates a TS file with no gql tags
        let parse = Parse {
            blocks: vec![],
            errors: vec![],
        };

        assert!(parse.is_empty());
        assert_eq!(parse.document_count(), 0);
        assert_eq!(parse.documents().count(), 0);
    }
}
