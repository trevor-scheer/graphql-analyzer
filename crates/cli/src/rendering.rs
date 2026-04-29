//! Diagnostic rendering with source code snippets using ariadne.
//!
//! Provides rich diagnostic output showing the relevant source code context
//! with span highlighting, replacing the simple `file:line:col: message` format
//! for human-readable output.

use ariadne::{Color, Label, Report, ReportKind, Source};
use graphql_ide::{Diagnostic, DiagnosticSeverity};

/// Convert 0-based line and character offsets to a byte offset in the source text.
fn line_col_to_byte_offset(source: &str, line: u32, character: u32) -> usize {
    let mut offset = 0;
    for (i, src_line) in source.lines().enumerate() {
        if i == line as usize {
            // Clamp character to line length to avoid out-of-bounds
            let char_offset = (character as usize).min(src_line.len());
            return offset + char_offset;
        }
        // +1 for the newline character
        offset += src_line.len() + 1;
    }
    // If line is beyond the end of the file, return the end
    source.len()
}

/// Render a diagnostic with source code context using ariadne.
///
/// Writes the rendered diagnostic to stderr (ariadne's `eprint` convention).
/// Returns `true` if the diagnostic was rendered, `false` if it could not be
/// (e.g., the range was invalid), in which case the caller should fall back
/// to the simple format.
pub fn render_diagnostic(file_path: &str, source_text: &str, diagnostic: &Diagnostic) -> bool {
    let kind = match diagnostic.severity {
        DiagnosticSeverity::Error => ReportKind::Error,
        DiagnosticSeverity::Warning => ReportKind::Warning,
        DiagnosticSeverity::Information | DiagnosticSeverity::Hint => ReportKind::Advice,
    };

    let start_offset = line_col_to_byte_offset(
        source_text,
        diagnostic.range.start.line,
        diagnostic.range.start.character,
    );
    let end_offset = line_col_to_byte_offset(
        source_text,
        diagnostic.range.end.line,
        diagnostic.range.end.character,
    );

    // Ensure we have a valid range
    let end_offset = end_offset.max(start_offset);

    let label_color = match diagnostic.severity {
        DiagnosticSeverity::Error => Color::Red,
        DiagnosticSeverity::Warning => Color::Yellow,
        DiagnosticSeverity::Information | DiagnosticSeverity::Hint => Color::Blue,
    };

    let label_message = diagnostic
        .code
        .as_deref()
        .unwrap_or(&diagnostic.message)
        .to_string();

    let mut builder = Report::build(kind, (file_path, start_offset..end_offset))
        .with_message(&diagnostic.message);

    if let Some(ref code) = diagnostic.code {
        builder = builder.with_code(code);
    }

    builder = builder.with_label(
        Label::new((file_path, start_offset..end_offset))
            .with_message(label_message)
            .with_color(label_color),
    );

    let report = builder.finish();

    // Write to a buffer first, then print to stdout for consistency with the
    // rest of the CLI output (which uses println! to stdout).
    let mut buf = Vec::new();
    if report
        .write((file_path, Source::from(source_text)), &mut buf)
        .is_err()
    {
        return false;
    }

    // Print the rendered diagnostic
    print!("{}", String::from_utf8_lossy(&buf));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_col_to_byte_offset_first_line() {
        let source = "hello world\nsecond line\n";
        assert_eq!(line_col_to_byte_offset(source, 0, 0), 0);
        assert_eq!(line_col_to_byte_offset(source, 0, 5), 5);
    }

    #[test]
    fn test_line_col_to_byte_offset_second_line() {
        let source = "hello world\nsecond line\n";
        // "hello world\n" = 12 bytes, so line 1, col 0 = offset 12
        assert_eq!(line_col_to_byte_offset(source, 1, 0), 12);
        assert_eq!(line_col_to_byte_offset(source, 1, 6), 18);
    }

    #[test]
    fn test_line_col_to_byte_offset_clamped_to_line_length() {
        let source = "short\n";
        // col 100 should clamp to end of "short" (len 5)
        assert_eq!(line_col_to_byte_offset(source, 0, 100), 5);
    }

    #[test]
    fn test_line_col_to_byte_offset_beyond_end() {
        let source = "hello\n";
        // line 99 is beyond the file
        assert_eq!(line_col_to_byte_offset(source, 99, 0), source.len());
    }

    #[test]
    fn test_render_diagnostic_produces_output() {
        use graphql_ide::{Position, Range};

        let source = "query GetUser {\n  user {\n    id\n  }\n}\n";
        let diagnostic = Diagnostic::new(
            Range::new(Position::new(1, 2), Position::new(1, 6)),
            DiagnosticSeverity::Warning,
            "test warning message",
            "test",
        );

        let result = render_diagnostic("test.graphql", source, &diagnostic);
        assert!(result, "render_diagnostic should succeed");
    }
}
