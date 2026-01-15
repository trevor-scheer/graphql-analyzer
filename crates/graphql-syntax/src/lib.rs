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

use graphql_db::{FileContent, FileKind, FileMetadata};
use std::sync::Arc;

/// A parse error with position information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Error message
    pub message: String,
    /// Byte offset where the error occurred
    pub offset: usize,
}

/// Result of parsing a file
///
/// All GraphQL content is represented uniformly as blocks. Pure GraphQL files
/// have a single block at offset 0. Use `documents()` to iterate over blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub line: usize,
    /// Column number in the original file (0-based)
    pub column: usize,
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
    pub line_offset: usize,
    /// Column offset in the original file (0 for pure GraphQL files)
    pub column_offset: usize,
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
            column_offset: block.column,
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
    match metadata.kind(db) {
        FileKind::Schema | FileKind::ExecutableGraphQL => {
            parse_graphql(&content.text(db), uri.as_str())
        }
        FileKind::TypeScript | FileKind::JavaScript => {
            extract_and_parse(db, &content.text(db), uri.as_str())
        }
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
        column: 0,
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
            column: block.location.range.start.column,
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

/// Determine `FileKind` for files opened/changed in the editor
///
/// For TypeScript/JavaScript files, returns the appropriate `FileKind` without content inspection.
/// For .graphql/.gql files, inspects the content to determine if it contains schema definitions
/// (`FileKind::Schema`) or executable documents (`FileKind::ExecutableGraphQL`).
#[must_use]
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub fn determine_file_kind_from_content(path: &str, content: &str) -> FileKind {
    if path.ends_with(".ts") || path.ends_with(".tsx") {
        return FileKind::TypeScript;
    }
    if path.ends_with(".js") || path.ends_with(".jsx") {
        return FileKind::JavaScript;
    }

    if content_has_schema_definitions(content) {
        FileKind::Schema
    } else {
        FileKind::ExecutableGraphQL
    }
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

#[salsa::db]
impl GraphQLSyntaxDatabase for graphql_db::RootDatabase {}

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
    fn test_determine_file_kind_typescript() {
        let content = "const query = gql`query { user { id } }`;";
        assert_eq!(
            determine_file_kind_from_content("file.ts", content),
            FileKind::TypeScript
        );
        assert_eq!(
            determine_file_kind_from_content("file.tsx", content),
            FileKind::TypeScript
        );
    }

    #[test]
    fn test_determine_file_kind_javascript() {
        let content = "const query = gql`query { user { id } }`;";
        assert_eq!(
            determine_file_kind_from_content("file.js", content),
            FileKind::JavaScript
        );
        assert_eq!(
            determine_file_kind_from_content("file.jsx", content),
            FileKind::JavaScript
        );
    }

    #[test]
    fn test_determine_file_kind_schema() {
        let content = "type User { id: ID! }";
        assert_eq!(
            determine_file_kind_from_content("schema.graphql", content),
            FileKind::Schema
        );
    }

    #[test]
    fn test_determine_file_kind_executable() {
        let content = "query GetUser { user { id } }";
        assert_eq!(
            determine_file_kind_from_content("query.graphql", content),
            FileKind::ExecutableGraphQL
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
                    column: 10,
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
                    column: 15,
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
