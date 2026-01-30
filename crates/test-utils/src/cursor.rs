//! Cursor position extraction for IDE feature tests.
//!
//! When testing IDE features like goto-definition, hover, or completion,
//! you need to specify a cursor position within the source. This module
//! provides helpers to mark positions with a `*` character and extract
//! the clean source and position.

/// A position in a document (0-indexed line and character).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// 0-indexed line number
    pub line: u32,
    /// 0-indexed character (column) number
    pub character: u32,
}

impl Position {
    /// Create a new position.
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Extract cursor position from source marked with `*`.
///
/// The `*` character marks where the cursor should be positioned. This function
/// removes the marker and returns both the clean source and the cursor position.
///
/// # Example
///
/// ```
/// use graphql_test_utils::extract_cursor;
///
/// // Single line
/// let (source, pos) = extract_cursor("query { user*Name }");
/// assert_eq!(source, "query { userName }");
/// assert_eq!(pos.line, 0);
/// assert_eq!(pos.character, 12);
///
/// // Multiline
/// let (source, pos) = extract_cursor("query {\n  *user\n}");
/// assert_eq!(source, "query {\n  user\n}");
/// assert_eq!(pos.line, 1);
/// assert_eq!(pos.character, 2);
/// ```
///
/// # Panics
///
/// Panics if the input contains no `*` marker or multiple `*` markers.
pub fn extract_cursor(input: &str) -> (String, Position) {
    let marker_count = input.chars().filter(|&c| c == '*').count();

    assert!(
        marker_count != 0,
        "extract_cursor: input must contain exactly one '*' marker, found none"
    );
    assert!(
        marker_count <= 1,
        "extract_cursor: input must contain exactly one '*' marker, found {marker_count}"
    );

    let mut line = 0u32;
    let mut character = 0u32;
    let mut found_pos = None;
    let mut result = String::with_capacity(input.len() - 1);

    for ch in input.chars() {
        if ch == '*' {
            found_pos = Some(Position::new(line, character));
        } else {
            result.push(ch);
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }
    }

    (result, found_pos.unwrap())
}

/// Extract multiple cursor positions from source marked with numbered markers.
///
/// Use `$1`, `$2`, etc. to mark multiple positions. Returns the clean source
/// and a vector of positions in marker order.
///
/// # Example
///
/// ```
/// use graphql_test_utils::extract_cursors;
///
/// let (source, positions) = extract_cursors("query { $1user { $2id } }");
/// assert_eq!(source, "query { user { id } }");
/// assert_eq!(positions.len(), 2);
/// ```
pub fn extract_cursors(input: &str) -> (String, Vec<Position>) {
    let mut positions: Vec<(usize, Position)> = Vec::new();
    let mut result = String::with_capacity(input.len());
    let mut line = 0u32;
    let mut character = 0u32;

    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            if let Some(&digit) = chars.peek() {
                if digit.is_ascii_digit() {
                    chars.next(); // consume the digit
                    let marker_num = digit.to_digit(10).unwrap() as usize;
                    positions.push((marker_num, Position::new(line, character)));
                    continue;
                }
            }
        }

        result.push(ch);
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    // Sort by marker number and extract just positions
    positions.sort_by_key(|(num, _)| *num);
    let positions: Vec<Position> = positions.into_iter().map(|(_, pos)| pos).collect();

    (result, positions)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_extract_cursor_end_of_file() {
        let (source, pos) = extract_cursor("query { user }*");
        assert_eq!(source, "query { user }");
        assert_eq!(pos, Position::new(0, 14));
    }

    #[test]
    #[should_panic(expected = "found none")]
    fn test_extract_cursor_no_marker() {
        extract_cursor("query { user }");
    }

    #[test]
    #[should_panic(expected = "found 2")]
    fn test_extract_cursor_multiple_markers() {
        extract_cursor("query { *user* }");
    }

    #[test]
    fn test_extract_cursors_multiple() {
        let (source, positions) = extract_cursors("query { $1user { $2id } }");
        assert_eq!(source, "query { user { id } }");
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], Position::new(0, 8)); // before "user"
        assert_eq!(positions[1], Position::new(0, 15)); // before "id"
    }

    #[test]
    fn test_extract_cursors_out_of_order() {
        let (source, positions) = extract_cursors("$2second $1first");
        assert_eq!(source, "second first");
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], Position::new(0, 7)); // $1 position
        assert_eq!(positions[1], Position::new(0, 0)); // $2 position
    }
}
