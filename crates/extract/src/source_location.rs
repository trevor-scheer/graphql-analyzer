//! Source location information for extracted GraphQL.

use graphql_types::Range;

/// Source location information for extracted GraphQL.
///
/// Contains both byte offsets (for text manipulation) and line/column
/// range (for display to users).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Byte offset in the original source file
    pub offset: usize,
    /// Length in bytes
    pub length: usize,
    /// Range in the original source file (line/column)
    pub range: Range,
}

impl SourceLocation {
    /// Create a new source location.
    #[must_use]
    pub const fn new(offset: usize, length: usize, range: Range) -> Self {
        Self {
            offset,
            length,
            range,
        }
    }
}
