// GraphQL Syntax Layer
// This crate handles parsing and syntax trees. No cross-file knowledge, no semantics.
// All parsing is file-local and fully parallelizable.

use graphql_db::{FileContent, FileKind, FileMetadata};
use std::sync::Arc;

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
    pub errors: Vec<String>,
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
            extract_and_parse(&content.text(db))
        }
    }
}

/// Parse pure GraphQL content
fn parse_graphql(content: &str) -> Parse {
    let parser = apollo_parser::Parser::new(content);
    let tree = parser.parse();

    let mut errors: Vec<String> = tree.errors().map(|e| e.message().to_string()).collect();

    // Parse with apollo-compiler to get AST
    let ast = match apollo_compiler::ast::Document::parse(content, "document.graphql") {
        Ok(doc) => doc,
        Err(with_errors) => {
            // Collect parse errors from apollo-compiler
            errors.extend(with_errors.errors.iter().map(|e| e.to_string()));
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
fn extract_and_parse(content: &str) -> Parse {
    use graphql_extract::{extract_from_source, ExtractConfig, Language};

    tracing::debug!(content_len = content.len(), "extract_and_parse called");

    let config = ExtractConfig::default();
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
        all_errors.extend(main_tree.errors().map(|e| e.message().to_string()));
    }

    // Parse each extracted block
    for block in extracted {
        let parser = apollo_parser::Parser::new(&block.source);
        let tree = parser.parse();

        // Collect errors for this block
        all_errors.extend(tree.errors().map(|e| e.message().to_string()));

        // Parse with apollo-compiler to get AST
        let ast = match apollo_compiler::ast::Document::parse(&block.source, "document.graphql") {
            Ok(doc) => doc,
            Err(with_errors) => {
                // Collect parse errors from apollo-compiler
                all_errors.extend(with_errors.errors.iter().map(|e| e.to_string()));
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
pub trait GraphQLSyntaxDatabase: salsa::Database {}

// Implement the trait for RootDatabase
// This makes RootDatabase usable with all syntax queries
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
}
