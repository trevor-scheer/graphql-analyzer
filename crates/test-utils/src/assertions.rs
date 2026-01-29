//! Snapshot testing assertions for GraphQL diagnostics
//!
//! This module provides helpers for snapshot testing with insta.
//! Diagnostics are formatted consistently for readable snapshots.

/// Format a list of diagnostics for snapshot testing.
///
/// # Example
///
/// ```ignore
/// use graphql_test_utils::assertions::format_diagnostics;
///
/// let diagnostics = validate_file(&db, content, metadata, project_files);
/// insta::assert_snapshot!(format_diagnostics(&diagnostics));
/// ```
pub fn format_diagnostics<D: std::fmt::Debug>(diagnostics: &[D]) -> String {
    if diagnostics.is_empty() {
        return String::from("(no diagnostics)");
    }

    diagnostics
        .iter()
        .enumerate()
        .map(|(i, d)| format!("[{}] {d:?}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format diagnostics with their messages only (without position info).
/// Useful when you only care about the error messages, not positions.
pub fn format_diagnostic_messages<T: AsRef<str>>(messages: &[T]) -> String {
    if messages.is_empty() {
        return String::from("(no diagnostics)");
    }

    messages
        .iter()
        .enumerate()
        .map(|(i, m)| format!("[{}] {}", i + 1, m.as_ref()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_diagnostics_empty() {
        let empty: Vec<String> = vec![];
        assert_eq!(format_diagnostics(&empty), "(no diagnostics)");
    }

    #[test]
    fn test_format_diagnostics_single() {
        let diagnostics = vec!["Error: unknown field"];
        let formatted = format_diagnostics(&diagnostics);
        assert!(formatted.contains("[1]"));
        assert!(formatted.contains("unknown field"));
    }

    #[test]
    fn test_format_diagnostic_messages() {
        let messages = vec!["Error 1", "Error 2"];
        let formatted = format_diagnostic_messages(&messages);
        assert_eq!(formatted, "[1] Error 1\n[2] Error 2");
    }
}
