//! Shared helper functions for IDE features.
//!
//! This module contains utility functions used across multiple IDE features
//! for position/offset conversion, range adjustment, and type formatting.

use crate::symbol::{
    find_fragment_definition_range, find_fragment_spreads, find_type_definition_range,
    find_type_references_in_tree,
};
use crate::types::{Position, Range};

/// Convert IDE position to byte offset using `LineIndex`
pub fn position_to_offset(
    line_index: &graphql_syntax::LineIndex,
    position: Position,
) -> Option<usize> {
    let line_start = line_index.line_start(position.line as usize)?;
    Some(line_start + position.character as usize)
}

/// Convert byte offset to IDE Position using `LineIndex`
#[allow(clippy::cast_possible_truncation)]
pub fn offset_to_position(line_index: &graphql_syntax::LineIndex, offset: usize) -> Position {
    let line = line_index.line_col(offset).0;
    let line_start = line_index.line_start(line).unwrap_or(0);
    let character = offset - line_start;
    Position::new(line as u32, character as u32)
}

/// Convert byte offset range to IDE Range using `LineIndex`
pub fn offset_range_to_range(
    line_index: &graphql_syntax::LineIndex,
    start_offset: usize,
    end_offset: usize,
) -> Range {
    let start = offset_to_position(line_index, start_offset);
    let end = offset_to_position(line_index, end_offset);
    Range::new(start, end)
}

/// Add line offset to a range (used when returning positions from extracted GraphQL)
///
/// When returning positions for document symbols in TypeScript/JavaScript files,
/// we need to add the `line_offset` to convert from GraphQL-relative positions
/// back to original file positions.
pub const fn adjust_range_for_line_offset(range: Range, line_offset: u32) -> Range {
    if line_offset == 0 {
        return range;
    }

    Range::new(
        Position::new(range.start.line + line_offset, range.start.character),
        Position::new(range.end.line + line_offset, range.end.character),
    )
}

/// Convert analysis Position to IDE Position
pub const fn convert_position(pos: graphql_analysis::Position) -> Position {
    Position {
        line: pos.line,
        character: pos.character,
    }
}

/// Convert analysis `DiagnosticRange` to IDE Range
pub const fn convert_range(range: graphql_analysis::DiagnosticRange) -> Range {
    Range {
        start: convert_position(range.start),
        end: convert_position(range.end),
    }
}

/// Convert analysis Severity to IDE `DiagnosticSeverity`
pub const fn convert_severity(
    severity: graphql_analysis::Severity,
) -> crate::types::DiagnosticSeverity {
    match severity {
        graphql_analysis::Severity::Error => crate::types::DiagnosticSeverity::Error,
        graphql_analysis::Severity::Warning => crate::types::DiagnosticSeverity::Warning,
        graphql_analysis::Severity::Info => crate::types::DiagnosticSeverity::Information,
    }
}

/// Convert analysis Diagnostic to IDE Diagnostic
pub fn convert_diagnostic(diag: &graphql_analysis::Diagnostic) -> crate::types::Diagnostic {
    crate::types::Diagnostic {
        range: convert_range(diag.range),
        severity: convert_severity(diag.severity),
        message: diag.message.to_string(),
        code: diag.code.as_ref().map(ToString::to_string),
        source: diag.source.to_string(),
        fix: None, // Fixes are handled separately via lint_diagnostics_with_fixes
    }
}

/// Result of finding which block contains a position
pub struct BlockContext<'a> {
    /// The syntax tree for the block (or main document)
    pub tree: &'a apollo_parser::SyntaxTree,
    /// Line offset to add when returning positions (0 for pure GraphQL files)
    pub line_offset: u32,
    /// The block source for building `LineIndex`
    pub block_source: &'a str,
}

/// Find which GraphQL block contains the given position
///
/// Iterates through all documents to find the one containing the cursor position.
/// For pure GraphQL files (single document at `line_offset` 0), the position maps directly.
/// For TS/JS files (multiple documents at various offsets), finds the block
/// containing the position and adjusts accordingly.
#[allow(clippy::cast_possible_truncation)]
pub fn find_block_for_position(
    parse: &graphql_syntax::Parse,
    position: Position,
) -> Option<(BlockContext<'_>, Position)> {
    // Iterate through all documents to find the one containing the position
    for doc in parse.documents() {
        let doc_start_line = doc.line_offset as u32;
        let doc_start_col = doc.column_offset as u32;
        let doc_lines = doc.source.chars().filter(|&c| c == '\n').count() as u32;

        if position.line >= doc_start_line && position.line <= doc_start_line + doc_lines {
            let adjusted_line = position.line - doc_start_line;
            let adjusted_col = if adjusted_line == 0 && doc_start_line > 0 {
                position.character.saturating_sub(doc_start_col)
            } else {
                position.character
            };
            let adjusted_pos = Position::new(adjusted_line, adjusted_col);

            return Some((
                BlockContext {
                    tree: doc.tree,
                    line_offset: doc_start_line,
                    block_source: doc.source,
                },
                adjusted_pos,
            ));
        }
    }

    None
}

/// Find a fragment definition in a parsed file, handling all document types uniformly
#[allow(clippy::cast_possible_truncation, unused_variables)]
pub fn find_fragment_definition_in_parse(
    parse: &graphql_syntax::Parse,
    fragment_name: &str,
    content: graphql_base_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
) -> Option<Range> {
    for doc in parse.documents() {
        if let Some((start_offset, end_offset)) =
            find_fragment_definition_range(doc.tree, fragment_name)
        {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            let range = offset_range_to_range(&line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, doc.line_offset as u32));
        }
    }

    None
}

/// Find a type definition in a parsed file, handling all document types uniformly
#[allow(clippy::cast_possible_truncation, unused_variables)]
pub fn find_type_definition_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    content: graphql_base_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
) -> Option<Range> {
    for doc in parse.documents() {
        if let Some((start_offset, end_offset)) = find_type_definition_range(doc.tree, type_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            let range = offset_range_to_range(&line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, doc.line_offset as u32));
        }
    }

    None
}

/// Find all fragment spreads in a parsed file, handling all document types uniformly
#[allow(clippy::cast_possible_truncation, unused_variables)]
pub fn find_fragment_spreads_in_parse(
    parse: &graphql_syntax::Parse,
    fragment_name: &str,
    content: graphql_base_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        if let Some(offsets) = find_fragment_spreads(doc.tree, fragment_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            for offset in offsets {
                let end_offset = offset + fragment_name.len();
                let range = offset_range_to_range(&line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, doc.line_offset as u32));
            }
        }
    }

    results
}

/// Find all type references in a parsed file, handling all document types uniformly
#[allow(clippy::cast_possible_truncation, unused_variables)]
pub fn find_type_references_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    content: graphql_base_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        if let Some(offsets) = find_type_references_in_tree(doc.tree, type_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            for offset in offsets {
                let end_offset = offset + type_name.len();
                let range = offset_range_to_range(&line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, doc.line_offset as u32));
            }
        }
    }

    results
}

/// Find field usages in a parsed file that match the given type and field name
#[allow(clippy::cast_possible_truncation, unused_variables)]
pub fn find_field_usages_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    field_name: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
    content: graphql_base_db::FileContent,
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        let line_index = graphql_syntax::LineIndex::new(doc.source);
        let ranges = find_field_usages_in_tree(doc.tree, type_name, field_name, schema_types);
        for (start, end) in ranges {
            let range = offset_range_to_range(&line_index, start, end);
            results.push(adjust_range_for_line_offset(range, doc.line_offset as u32));
        }
    }

    results
}

/// Check if `current_type` matches `target_type` directly or implements it as an interface
fn type_matches_or_implements(
    current_type: &str,
    target_type: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> bool {
    if current_type == target_type {
        return true;
    }
    if let Some(type_def) = schema_types.get(current_type) {
        type_def
            .implements
            .iter()
            .any(|i| i.as_ref() == target_type)
    } else {
        false
    }
}

/// Find all field usages in a tree that match the given type and field name
#[allow(clippy::too_many_lines)]
pub fn find_field_usages_in_tree(
    tree: &apollo_parser::SyntaxTree,
    target_type: &str,
    target_field: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> Vec<(usize, usize)> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn search_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        current_type: &str,
        target_type: &str,
        target_field: &str,
        schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
        results: &mut Vec<(usize, usize)>,
    ) {
        for selection in selection_set.selections() {
            match selection {
                Selection::Field(field) => {
                    if let Some(name) = field.name() {
                        let field_name = name.text();

                        if type_matches_or_implements(current_type, target_type, schema_types)
                            && field_name == target_field
                        {
                            let range = name.syntax().text_range();
                            results.push((range.start().into(), range.end().into()));
                        }

                        if let Some(nested) = field.selection_set() {
                            if let Some(type_def) = schema_types.get(current_type) {
                                if let Some(field_def) = type_def
                                    .fields
                                    .iter()
                                    .find(|f| f.name.as_ref() == field_name)
                                {
                                    let field_type = field_def.type_ref.name.as_ref();
                                    search_selection_set(
                                        &nested,
                                        field_type,
                                        target_type,
                                        target_field,
                                        schema_types,
                                        results,
                                    );
                                }
                            }
                        }
                    }
                }
                Selection::InlineFragment(inline_frag) => {
                    let fragment_type = inline_frag
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| current_type.to_string(), |n| n.text().to_string());

                    if let Some(nested) = inline_frag.selection_set() {
                        search_selection_set(
                            &nested,
                            &fragment_type,
                            target_type,
                            target_field,
                            schema_types,
                            results,
                        );
                    }
                }
                Selection::FragmentSpread(_) => {}
            }
        }
    }

    let mut results = Vec::new();
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                let root_type = match op.operation_type() {
                    Some(op_type) if op_type.mutation_token().is_some() => "Mutation",
                    Some(op_type) if op_type.subscription_token().is_some() => "Subscription",
                    _ => "Query",
                };

                if let Some(selection_set) = op.selection_set() {
                    search_selection_set(
                        &selection_set,
                        root_type,
                        target_type,
                        target_field,
                        schema_types,
                        &mut results,
                    );
                }
            }
            Definition::FragmentDefinition(frag) => {
                let fragment_type = frag
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|n| n.text().to_string());

                if let (Some(fragment_type), Some(selection_set)) =
                    (fragment_type, frag.selection_set())
                {
                    search_selection_set(
                        &selection_set,
                        &fragment_type,
                        target_type,
                        target_field,
                        schema_types,
                        &mut results,
                    );
                }
            }
            _ => {}
        }
    }

    results
}

/// Find variable definition in an operation by name
pub fn find_variable_definition_in_tree(
    tree: &apollo_parser::SyntaxTree,
    var_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        if let Definition::OperationDefinition(op) = definition {
            if let Some(var_defs) = op.variable_definitions() {
                for var_def in var_defs.variable_definitions() {
                    if let Some(variable) = var_def.variable() {
                        if let Some(name) = variable.name() {
                            if name.text() == var_name {
                                let range = name.syntax().text_range();
                                let start: usize = range.start().into();
                                let end: usize = range.end().into();
                                let pos_range = offset_range_to_range(line_index, start, end);
                                return Some(adjust_range_for_line_offset(pos_range, line_offset));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find operation definition by name
pub fn find_operation_definition_in_tree(
    tree: &apollo_parser::SyntaxTree,
    op_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        if let Definition::OperationDefinition(op) = definition {
            if let Some(name) = op.name() {
                if name.text() == op_name {
                    let range = name.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    let pos_range = offset_range_to_range(line_index, start, end);
                    return Some(adjust_range_for_line_offset(pos_range, line_offset));
                }
            }
        }
    }
    None
}

/// Find argument definition in schema type's field
pub fn find_argument_definition_in_tree(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
    field_name: &str,
    arg_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        let (name_node, fields_def) = match &definition {
            Definition::ObjectTypeDefinition(obj) => (obj.name(), obj.fields_definition()),
            Definition::InterfaceTypeDefinition(iface) => (iface.name(), iface.fields_definition()),
            _ => continue,
        };

        let Some(name) = name_node else { continue };
        if name.text() != type_name {
            continue;
        }

        let Some(fields) = fields_def else { continue };
        for field in fields.field_definitions() {
            let Some(fname) = field.name() else { continue };
            if fname.text() != field_name {
                continue;
            }

            if let Some(args_def) = field.arguments_definition() {
                for input_val in args_def.input_value_definitions() {
                    if let Some(aname) = input_val.name() {
                        if aname.text() == arg_name {
                            let range = aname.syntax().text_range();
                            let start: usize = range.start().into();
                            let end: usize = range.end().into();
                            let pos_range = offset_range_to_range(line_index, start, end);
                            return Some(adjust_range_for_line_offset(pos_range, line_offset));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find the field name at a given offset (for argument context)
pub fn find_field_name_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<String> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn check_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        byte_offset: usize,
    ) -> Option<String> {
        for selection in selection_set.selections() {
            if let Selection::Field(field) = selection {
                let range = field.syntax().text_range();
                let start: usize = range.start().into();
                let end: usize = range.end().into();

                if byte_offset >= start && byte_offset <= end {
                    if let Some(args) = field.arguments() {
                        let args_range = args.syntax().text_range();
                        let args_start: usize = args_range.start().into();
                        let args_end: usize = args_range.end().into();
                        if byte_offset >= args_start && byte_offset <= args_end {
                            return field.name().map(|n| n.text().to_string());
                        }
                    }

                    if let Some(nested) = field.selection_set() {
                        if let Some(name) = check_selection_set(&nested, byte_offset) {
                            return Some(name);
                        }
                    }
                }
            }
        }
        None
    }

    let doc = tree.document();
    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    if let Some(name) = check_selection_set(&selection_set, byte_offset) {
                        return Some(name);
                    }
                }
            }
            Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    if let Some(name) = check_selection_set(&selection_set, byte_offset) {
                        return Some(name);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Unwrap a `TypeRef` to get just the base type name (without List or `NonNull` wrappers)
#[must_use]
pub fn unwrap_type_to_name(type_ref: &graphql_hir::TypeRef) -> String {
    type_ref.name.to_string()
}

/// Format a type reference for display (e.g., "[String!]!")
pub fn format_type_ref(type_ref: &graphql_hir::TypeRef) -> String {
    let mut result = type_ref.name.to_string();

    if type_ref.is_list {
        if type_ref.inner_non_null {
            result.push('!');
        }
        result = format!("[{result}]");
    }

    if type_ref.is_non_null {
        result.push('!');
    }

    result
}

/// Convert a filesystem path to a file:// URI
///
/// Handles both Unix and Windows paths:
/// - Unix: `/path/to/file` -> `file:///path/to/file`
/// - Windows: `C:\path\to\file` -> `file:///C:/path/to/file`
#[must_use]
pub fn path_to_file_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();

    if path_str.starts_with("file://") || path_str.contains("://") {
        return path_str.to_string();
    }

    // Unix path (starts with /)
    if path_str.starts_with('/') {
        return format!("file://{path_str}");
    }

    // Windows path (e.g., C:\Users\...) - convert to file:///C:/Users/...
    // Check for drive letter pattern (e.g., "C:" or "D:")
    let chars: Vec<char> = path_str.chars().collect();
    if chars.len() >= 2 && chars[0].is_ascii_alphabetic() && chars[1] == ':' {
        // Convert backslashes to forward slashes for proper URI format
        let normalized = path_str.replace('\\', "/");
        return format!("file:///{normalized}");
    }

    path_str.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_offset_helper() {
        let text = "line 1\nline 2\nline 3";
        let line_index = graphql_syntax::LineIndex::new(text);

        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 0)),
            Some(0)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 5)),
            Some(5)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(1, 0)),
            Some(7)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(1, 3)),
            Some(10)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(2, 0)),
            Some(14)
        );
    }

    #[test]
    fn test_conversion_position() {
        let analysis_pos = graphql_analysis::Position::new(10, 20);
        let ide_pos = convert_position(analysis_pos);

        assert_eq!(ide_pos.line, 10);
        assert_eq!(ide_pos.character, 20);
    }

    #[test]
    fn test_conversion_range() {
        let analysis_range = graphql_analysis::DiagnosticRange::new(
            graphql_analysis::Position::new(1, 5),
            graphql_analysis::Position::new(1, 10),
        );
        let ide_range = convert_range(analysis_range);

        assert_eq!(ide_range.start.line, 1);
        assert_eq!(ide_range.start.character, 5);
        assert_eq!(ide_range.end.line, 1);
        assert_eq!(ide_range.end.character, 10);
    }

    #[test]
    fn test_conversion_severity() {
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Error),
            crate::types::DiagnosticSeverity::Error
        );
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Warning),
            crate::types::DiagnosticSeverity::Warning
        );
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Info),
            crate::types::DiagnosticSeverity::Information
        );
    }

    #[test]
    fn test_conversion_diagnostic() {
        let analysis_diag = graphql_analysis::Diagnostic::with_source_and_code(
            graphql_analysis::Severity::Warning,
            "Test warning message",
            graphql_analysis::DiagnosticRange::new(
                graphql_analysis::Position::new(2, 0),
                graphql_analysis::Position::new(2, 10),
            ),
            "test-source",
            "TEST001",
        );

        let ide_diag = convert_diagnostic(&analysis_diag);

        assert_eq!(ide_diag.severity, crate::types::DiagnosticSeverity::Warning);
        assert_eq!(ide_diag.message, "Test warning message");
        assert_eq!(ide_diag.source, "test-source");
        assert_eq!(ide_diag.code, Some("TEST001".to_string()));
        assert_eq!(ide_diag.range.start.line, 2);
        assert_eq!(ide_diag.range.start.character, 0);
        assert_eq!(ide_diag.range.end.line, 2);
        assert_eq!(ide_diag.range.end.character, 10);
    }

    #[test]
    fn test_path_to_file_uri_unix() {
        use std::path::Path;

        // Unix absolute path
        assert_eq!(
            path_to_file_uri(Path::new("/home/user/file.graphql")),
            "file:///home/user/file.graphql"
        );

        // Unix nested path
        assert_eq!(
            path_to_file_uri(Path::new("/var/lib/app/schema.graphql")),
            "file:///var/lib/app/schema.graphql"
        );
    }

    #[test]
    fn test_path_to_file_uri_already_uri() {
        use std::path::Path;

        // Already a file URI - should pass through unchanged
        assert_eq!(
            path_to_file_uri(Path::new("file:///home/user/file.graphql")),
            "file:///home/user/file.graphql"
        );

        // Other URI scheme - should pass through unchanged
        assert_eq!(
            path_to_file_uri(Path::new("https://example.com/schema")),
            "https://example.com/schema"
        );
    }

    #[test]
    fn test_path_to_file_uri_windows_style() {
        // Test Windows-style paths with backslashes
        // On Windows, Path::new will properly parse this as a Windows path
        #[cfg(windows)]
        {
            let windows_path = "C:\\Users\\test\\schema.graphql";
            let result = path_to_file_uri(std::path::Path::new(windows_path));
            assert_eq!(result, "file:///C:/Users/test/schema.graphql");
        }

        // Test drive letter detection with forward slashes (cross-platform)
        // This tests the drive letter detection logic on all platforms
        let path_with_drive = "D:/Projects/app/query.graphql";
        let result = path_to_file_uri(std::path::Path::new(path_with_drive));
        assert_eq!(result, "file:///D:/Projects/app/query.graphql");
    }
}
