//! Position and range types for source locations.

use std::sync::Arc;

/// Byte offset range in a source file.
///
/// Used internally for efficient text manipulation. Byte offsets are
/// converted to line/column [`Position`]s when presenting to users or LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OffsetRange {
    /// Start byte offset (inclusive)
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
}

impl OffsetRange {
    /// Create a new offset range.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Create a zero-width range at a specific offset.
    #[must_use]
    pub const fn at(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Returns the length of this range in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    /// Returns `true` if this is a zero-width range.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl std::fmt::Display for OffsetRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// Position in a source file (editor coordinates, 0-indexed).
///
/// This represents a position as understood by editors and LSP:
/// - `line` is 0-indexed (first line is 0)
/// - `character` is 0-indexed UTF-16 code units from line start
///
/// Note: The LSP specification uses UTF-16 code units for character offsets,
/// not bytes or Unicode codepoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    /// Line number (0-indexed)
    pub line: u32,
    /// Character offset within the line (0-indexed, UTF-16 code units)
    pub character: u32,
}

impl Position {
    /// Create a new position.
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.line.cmp(&other.line) {
            std::cmp::Ordering::Equal => self.character.cmp(&other.character),
            ord => ord,
        }
    }
}

/// Range in a source file (editor coordinates).
///
/// A range represents a span of text from `start` (inclusive) to `end` (exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Range {
    /// Start position (inclusive)
    pub start: Position,
    /// End position (exclusive)
    pub end: Position,
}

impl Range {
    /// Create a new range.
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Create a zero-width range at a specific position.
    #[must_use]
    pub const fn at(position: Position) -> Self {
        Self {
            start: position,
            end: position,
        }
    }

    /// Returns `true` if this is a zero-width range.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start.line == self.end.line && self.start.character == self.end.character
    }

    /// Check if this range contains a position.
    #[must_use]
    pub fn contains(&self, position: Position) -> bool {
        position >= self.start && position < self.end
    }
}

/// A byte offset range within a GraphQL source block, with the block's position
/// context for correct mapping back to the original file.
///
/// For pure `.graphql` files, `line_offset` and `byte_offset` are 0 and `source` is `None`.
/// For GraphQL extracted from TS/JS template literals, these fields describe where the
/// block starts in the original file, enabling correct line/column calculation.
///
/// Use [`DocumentRef::span()`](graphql_syntax::DocumentRef::span) to create spans --
/// it automatically fills in block context from the document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SourceSpan {
    /// Start byte offset within the block (inclusive)
    pub start: usize,
    /// End byte offset within the block (exclusive)
    pub end: usize,
    /// Line offset of the block in the original file (0-based, 0 for pure GraphQL)
    pub line_offset: u32,
    /// Byte offset of the block in the original file (0 for pure GraphQL)
    pub byte_offset: usize,
    /// The block's source text (None for pure GraphQL -- use the full file instead)
    pub source: Option<Arc<str>>,
}

impl SourceSpan {
    /// Create a span with block context from HIR or other sources.
    ///
    /// Prefer `DocumentRef::span()` when a `DocumentRef` is available.
    #[must_use]
    pub fn with_block_context(
        start: usize,
        end: usize,
        line_offset: u32,
        byte_offset: usize,
        source: Option<Arc<str>>,
    ) -> Self {
        Self {
            start,
            end,
            line_offset,
            byte_offset,
            source,
        }
    }

    /// Returns the `OffsetRange` portion of this span.
    #[must_use]
    pub const fn offset_range(&self) -> OffsetRange {
        OffsetRange {
            start: self.start,
            end: self.end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_range_creation() {
        let range = OffsetRange::new(10, 20);
        assert_eq!(range.start, 10);
        assert_eq!(range.end, 20);
        assert_eq!(range.len(), 10);
        assert!(!range.is_empty());
    }

    #[test]
    fn test_offset_range_at() {
        let range = OffsetRange::at(15);
        assert_eq!(range.start, 15);
        assert_eq!(range.end, 15);
        assert_eq!(range.len(), 0);
        assert!(range.is_empty());
    }

    #[test]
    fn test_offset_range_display() {
        let range = OffsetRange::new(10, 20);
        assert_eq!(format!("{range}"), "10..20");
    }

    #[test]
    fn test_position_creation() {
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_position_ordering() {
        let p1 = Position::new(0, 5);
        let p2 = Position::new(0, 10);
        let p3 = Position::new(1, 0);

        assert!(p1 < p2);
        assert!(p2 < p3);
        assert!(p1 < p3);
        assert_eq!(p1.cmp(&p1), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_range_creation() {
        let range = Range::new(Position::new(0, 0), Position::new(1, 10));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 1);
        assert!(!range.is_empty());
    }

    #[test]
    fn test_range_at() {
        let pos = Position::new(5, 10);
        let range = Range::at(pos);
        assert_eq!(range.start, pos);
        assert_eq!(range.end, pos);
        assert!(range.is_empty());
    }

    #[test]
    fn test_range_contains() {
        let range = Range::new(Position::new(1, 0), Position::new(3, 0));
        assert!(range.contains(Position::new(1, 5)));
        assert!(range.contains(Position::new(2, 0)));
        assert!(!range.contains(Position::new(0, 5)));
        assert!(!range.contains(Position::new(3, 0))); // end is exclusive
    }
}
