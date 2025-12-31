// GraphQL Syntax Layer
// This crate handles parsing and syntax trees. No cross-file knowledge, no semantics.
// All parsing is file-local and fully parallelizable.

//! # Migration Guide: Using `Parse::documents()`
//!
//! The `Parse` struct provides the `documents()` method for safely handling both
//! pure GraphQL files and TypeScript/JavaScript files with embedded GraphQL.
//!
//! ## The Problem
//!
//! Direct field access (`tree`, `ast`, `blocks`) requires manual if/else logic:
//!
//! ```rust,ignore
//! // ❌ Easy to get wrong
//! if parse.blocks.is_empty() {
//!     process(&parse.tree);
//! } else {
//!     for block in &parse.blocks {
//!         process(&block.tree);
//!     }
//! }
//! ```
//!
//! ## The Solution
//!
//! Use `documents()` for uniform iteration:
//!
//! ```rust,ignore
//! // ✅ Always correct
//! for doc in parse.documents() {
//!     process(doc.tree, doc.line_offset);
//! }
//! ```

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    /// The syntax tree (Arc for cheap cloning) - CST for position information
    pub tree: Arc<apollo_parser::SyntaxTree>,
    /// The AST (Arc for cheap cloning) - AST for semantic analysis
    pub ast: Arc<apollo_compiler::ast::Document>,
    /// For TypeScript/JavaScript: extracted GraphQL blocks
    pub blocks: Vec<ExtractedBlock>,
    /// Parse errors (syntax errors only, not validation)
    pub errors: Vec<ParseError>,
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
#[derive(Debug, Clone, Copy)]
pub struct DocumentRef<'a> {
    /// The syntax tree for this document
    pub tree: &'a apollo_parser::SyntaxTree,
    /// The AST for this document
    pub ast: &'a apollo_compiler::ast::Document,
    /// Line offset in the original file (0 for pure GraphQL files)
    pub line_offset: usize,
    /// The source code (None for pure GraphQL files, Some for extracted blocks)
    pub source: Option<&'a str>,
}

impl Parse {
    /// Returns an iterator over all GraphQL documents in this file.
    ///
    /// For pure GraphQL files, yields a single document.
    /// For TypeScript/JavaScript files, yields one document per extracted block.
    ///
    /// # Example
    /// ```ignore
    /// for doc in parse.documents() {
    ///     validate_document(doc.tree, doc.ast, doc.line_offset);
    /// }
    /// ```
    #[must_use]
    pub fn documents(&self) -> DocumentIterator<'_> {
        if self.blocks.is_empty() {
            // Pure GraphQL file - yield single document
            DocumentIterator {
                parse: self,
                state: IteratorState::Single(false),
            }
        } else {
            // TypeScript/JavaScript file - yield each block
            DocumentIterator {
                parse: self,
                state: IteratorState::Multiple(0),
            }
        }
    }
}

/// Iterator over documents in a parsed file
pub struct DocumentIterator<'a> {
    parse: &'a Parse,
    state: IteratorState,
}

enum IteratorState {
    Single(bool),    // bool tracks if we've yielded the single item
    Multiple(usize), // usize is the current block index
}

impl<'a> Iterator for DocumentIterator<'a> {
    type Item = DocumentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            IteratorState::Single(yielded) => {
                if *yielded {
                    None
                } else {
                    *yielded = true;
                    Some(DocumentRef {
                        tree: &self.parse.tree,
                        ast: &self.parse.ast,
                        line_offset: 0,
                        source: None,
                    })
                }
            }
            IteratorState::Multiple(index) => {
                if let Some(block) = self.parse.blocks.get(*index) {
                    *index += 1;
                    Some(DocumentRef {
                        tree: &block.tree,
                        ast: &block.ast,
                        line_offset: block.line,
                        source: Some(&block.source),
                    })
                } else {
                    None
                }
            }
        }
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
    match metadata.kind(db) {
        FileKind::Schema | FileKind::ExecutableGraphQL => {
            // Use apollo-parser for pure GraphQL files
            parse_graphql(&content.text(db))
        }
        FileKind::TypeScript | FileKind::JavaScript => {
            // Use graphql-extract to get blocks, then parse each
            extract_and_parse(db, &content.text(db))
        }
    }
}

/// Parse pure GraphQL content
fn parse_graphql(content: &str) -> Parse {
    let parser = apollo_parser::Parser::new(content);
    let tree = parser.parse();

    let mut errors: Vec<ParseError> = tree
        .errors()
        .map(|e| ParseError {
            message: e.message().to_string(),
            offset: e.index(),
        })
        .collect();

    // Parse with apollo-compiler to get AST
    let ast = match apollo_compiler::ast::Document::parse(content, "document.graphql") {
        Ok(doc) => doc,
        Err(with_errors) => {
            // Collect parse errors from apollo-compiler
            // Note: apollo-compiler errors don't have precise positions in the same way,
            // so we use offset 0 for these
            errors.extend(with_errors.errors.iter().map(|e| ParseError {
                message: e.to_string(),
                offset: 0,
            }));
            // Use the partial document even with errors
            with_errors.partial
        }
    };

    Parse {
        tree: Arc::new(tree),
        ast: Arc::new(ast),
        blocks: Vec::new(),
        errors,
    }
}

/// Extract GraphQL from TypeScript/JavaScript and parse each block
fn extract_and_parse(db: &dyn GraphQLSyntaxDatabase, content: &str) -> Parse {
    use graphql_extract::{extract_from_source, ExtractConfig, Language};

    tracing::debug!(content_len = content.len(), "extract_and_parse called");

    // Get extract config from database, or use default
    let config = db
        .extract_config()
        .map_or_else(ExtractConfig::default, |arc| (*arc).clone());

    tracing::debug!(
        allow_global_identifiers = config.allow_global_identifiers,
        tag_identifiers = ?config.tag_identifiers,
        "Using extract config"
    );

    let language = Language::TypeScript; // Will work for both TS and JS
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

    // For the main tree, we'll use the first block or create an empty document
    let (main_tree, main_ast) = extracted.first().map_or_else(
        || {
            let tree = apollo_parser::Parser::new("").parse();
            let ast = apollo_compiler::ast::Document::default();
            (tree, ast)
        },
        |first_block| {
            let parser = apollo_parser::Parser::new(&first_block.source);
            let tree = parser.parse();
            let ast = match apollo_compiler::ast::Document::parse(
                &first_block.source,
                "document.graphql",
            ) {
                Ok(doc) => doc,
                Err(with_errors) => with_errors.partial,
            };
            (tree, ast)
        },
    );

    // Collect errors from main tree (only if we actually extracted blocks)
    if !extracted.is_empty() {
        all_errors.extend(main_tree.errors().map(|e| ParseError {
            message: e.message().to_string(),
            offset: e.index(),
        }));
    }

    // Parse each extracted block
    for block in extracted {
        let parser = apollo_parser::Parser::new(&block.source);
        let tree = parser.parse();

        // Collect errors for this block, adjusting offsets to original file positions
        let block_offset = block.location.offset;
        all_errors.extend(tree.errors().map(|e| ParseError {
            message: e.message().to_string(),
            offset: block_offset + e.index(),
        }));

        // Parse with apollo-compiler to get AST
        let ast = match apollo_compiler::ast::Document::parse(&block.source, "document.graphql") {
            Ok(doc) => doc,
            Err(with_errors) => {
                // Collect parse errors from apollo-compiler
                // Use block offset for these errors since we don't have precise positions
                all_errors.extend(with_errors.errors.iter().map(|e| ParseError {
                    message: e.to_string(),
                    offset: block_offset,
                }));
                // Use the partial document even with errors
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
        tree: Arc::new(main_tree),
        ast: Arc::new(main_ast),
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
    // For TypeScript/JavaScript, return the appropriate FileKind
    if path.ends_with(".ts") || path.ends_with(".tsx") {
        return FileKind::TypeScript;
    }
    if path.ends_with(".js") || path.ends_with(".jsx") {
        return FileKind::JavaScript;
    }

    // For .graphql/.gql files, check content to determine Schema vs ExecutableGraphQL
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

        // First line
        assert_eq!(index.line_col(0), (0, 0));
        assert_eq!(index.line_col(5), (0, 5));

        // Second line
        assert_eq!(index.line_col(7), (1, 0));
        assert_eq!(index.line_col(10), (1, 3));

        // Third line
        assert_eq!(index.line_col(14), (2, 0));
    }

    #[test]
    fn test_parse_graphql() {
        let content = "type User { id: ID! }";
        let parse = parse_graphql(content);

        assert!(parse.errors.is_empty());
        assert!(parse.blocks.is_empty());
        assert_eq!(parse.tree.document().definitions().count(), 1);
    }

    #[test]
    fn test_parse_graphql_with_error() {
        let content = "type User {";
        let parse = parse_graphql(content);

        assert!(!parse.errors.is_empty());
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

        for error in tree.errors() {
            println!("Error message: {}", error.message());
            println!("Error data: {:?}", error.data());
            println!("Error index: {:?}", error.index());
        }

        // This test is just for exploration
        assert!(tree.errors().next().is_some());
    }

    #[test]
    fn test_documents_iterator_pure_graphql() {
        let content = "type User { id: ID! }\ntype Post { id: ID! }";
        let parse = parse_graphql(content);

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs.len(), 1);

        let doc = &docs[0];
        assert_eq!(doc.line_offset, 0);
        assert!(doc.source.is_none());
        assert_eq!(doc.tree.document().definitions().count(), 2);
    }

    #[test]
    fn test_documents_iterator_with_blocks() {
        let parse = Parse {
            tree: Arc::new(apollo_parser::Parser::new("").parse()),
            ast: Arc::new(apollo_compiler::ast::Document::default()),
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
                    column: 10,
                },
            ],
            errors: vec![],
        };

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs.len(), 2);

        assert_eq!(docs[0].line_offset, 5);
        assert_eq!(docs[0].source, Some("query Q1 { user { id } }"));

        assert_eq!(docs[1].line_offset, 10);
        assert_eq!(docs[1].source, Some("query Q2 { post { id } }"));
    }

    #[test]
    fn test_documents_iterator_empty_blocks() {
        // Edge case: empty blocks vector should behave like pure GraphQL
        let content = "type User { id: ID! }";
        let parse = parse_graphql(content);

        let docs: Vec<_> = parse.documents().collect();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].line_offset, 0);
    }
}
