use crate::{LineIndex, Position};

/// Convert a line/column position to a byte offset using a cached `LineIndex`
///
/// This is the fast path with O(1) complexity instead of O(N) character iteration.
///
/// Returns `None` if the position is out of bounds.
#[must_use]
pub fn position_to_offset_with_index(line_index: &LineIndex, position: Position) -> Option<usize> {
    line_index.position_to_offset(position)
}

/// Convert a line/column position to a byte offset (fallback O(N) implementation)
///
/// This is the slow path used when no cached `LineIndex` is available.
/// Prefers `position_to_offset_with_index` when possible.
///
/// Returns `None` if the position is beyond the end of the source.
#[must_use]
pub fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let mut current_line = 0;
    let mut current_col = 0;
    let mut offset = 0;

    for ch in source.chars() {
        if current_line == position.line && current_col == position.character {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }

        offset += ch.len_utf8();
    }

    if current_line == position.line && current_col == position.character {
        Some(offset)
    } else {
        None
    }
}

/// Convert a byte offset to a line and column (0-indexed)
///
/// This function iterates through the document character by character
/// until it reaches the specified offset, counting newlines to determine
/// the line number and characters to determine the column.
///
/// Returns a tuple of (line, column) both 0-indexed.
#[must_use]
pub fn offset_to_line_col(document: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for ch in document.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }

        current_offset += ch.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_offset() {
        let source = "hello\nworld";

        let offset = position_to_offset(
            source,
            Position {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(offset, Some(0));

        let offset = position_to_offset(
            source,
            Position {
                line: 1,
                character: 0,
            },
        );
        assert_eq!(offset, Some(6));

        let offset = position_to_offset(
            source,
            Position {
                line: 1,
                character: 5,
            },
        );
        assert_eq!(offset, Some(11));

        let offset = position_to_offset(
            source,
            Position {
                line: 2,
                character: 0,
            },
        );
        assert_eq!(offset, None);
    }

    #[test]
    fn test_position_to_offset_with_index() {
        let source = "hello\nworld";
        let index = LineIndex::new(source);

        let offset = position_to_offset_with_index(
            &index,
            Position {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(offset, Some(0));

        let offset = position_to_offset_with_index(
            &index,
            Position {
                line: 1,
                character: 0,
            },
        );
        assert_eq!(offset, Some(6));
    }

    #[test]
    fn test_offset_to_line_col() {
        let source = "hello\nworld";

        let (line, col) = offset_to_line_col(source, 0);
        assert_eq!((line, col), (0, 0));

        let (line, col) = offset_to_line_col(source, 6);
        assert_eq!((line, col), (1, 0));

        let (line, col) = offset_to_line_col(source, 8);
        assert_eq!((line, col), (1, 2));
    }

    #[test]
    fn test_offset_to_line_col_utf8() {
        let source = "hello 世界\nworld";

        let (line, col) = offset_to_line_col(source, 0);
        assert_eq!((line, col), (0, 0));

        let (line, col) = offset_to_line_col(source, 13);
        assert_eq!((line, col), (1, 0));
    }
}
