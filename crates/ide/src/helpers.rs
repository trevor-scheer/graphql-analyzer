//! Shared helper functions for IDE features.
//!
//! This module contains utility functions used across multiple IDE features
//! for position/offset conversion, range adjustment, and type formatting.

use crate::symbol::{
    find_fragment_definition_range, find_fragment_spreads, find_type_definition_range,
    find_type_references_in_tree,
};
use crate::types::{Position, Range};

/// Convert IDE position (UTF-16 columns) to byte offset using `LineIndex`
pub fn position_to_offset(
    line_index: &graphql_syntax::LineIndex,
    position: Position,
) -> Option<usize> {
    line_index.utf16_to_offset(position.line as usize, position.character)
}

/// Convert byte offset to IDE Position (UTF-16 columns) using `LineIndex`
pub fn offset_to_position(line_index: &graphql_syntax::LineIndex, offset: usize) -> Position {
    let (line, utf16_col) = line_index.line_col(offset);
    Position::new(line as u32, utf16_col as u32)
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
        help: diag.help.as_ref().map(ToString::to_string),
        url: diag.url.as_ref().map(ToString::to_string),
        tags: diag
            .tags
            .iter()
            .map(|t| match t {
                graphql_analysis::DiagnosticTag::Unnecessary => {
                    crate::types::DiagnosticTag::Unnecessary
                }
                graphql_analysis::DiagnosticTag::Deprecated => {
                    crate::types::DiagnosticTag::Deprecated
                }
            })
            .collect(),
        related: Vec::new(),
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
pub fn find_block_for_position(
    parse: &graphql_syntax::Parse,
    position: Position,
) -> Option<(BlockContext<'_>, Position)> {
    // Iterate through all documents to find the one containing the position
    for doc in parse.documents() {
        let doc_start_line = doc.line_offset;
        let doc_start_col = doc.column_offset;
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
pub fn find_fragment_definition_in_parse(
    parse: &graphql_syntax::Parse,
    fragment_name: &str,
) -> Option<Range> {
    for doc in parse.documents() {
        if let Some((start_offset, end_offset)) =
            find_fragment_definition_range(doc.tree, fragment_name)
        {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            let range = offset_range_to_range(&line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, doc.line_offset));
        }
    }

    None
}

/// Find a type definition in a parsed file, handling all document types uniformly
pub fn find_type_definition_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
) -> Option<Range> {
    for doc in parse.documents() {
        if let Some((start_offset, end_offset)) = find_type_definition_range(doc.tree, type_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            let range = offset_range_to_range(&line_index, start_offset, end_offset);
            return Some(adjust_range_for_line_offset(range, doc.line_offset));
        }
    }

    None
}

/// Find all fragment spreads in a parsed file, handling all document types uniformly
pub fn find_fragment_spreads_in_parse(
    parse: &graphql_syntax::Parse,
    fragment_name: &str,
) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        if let Some(offsets) = find_fragment_spreads(doc.tree, fragment_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            for offset in offsets {
                let end_offset = offset + fragment_name.len();
                let range = offset_range_to_range(&line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, doc.line_offset));
            }
        }
    }

    results
}

/// Find all type references in a parsed file, handling all document types uniformly
pub fn find_type_references_in_parse(parse: &graphql_syntax::Parse, type_name: &str) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        if let Some(offsets) = find_type_references_in_tree(doc.tree, type_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            for offset in offsets {
                let end_offset = offset + type_name.len();
                let range = offset_range_to_range(&line_index, offset, end_offset);
                results.push(adjust_range_for_line_offset(range, doc.line_offset));
            }
        }
    }

    results
}

/// Find field usages in a parsed file that match the given type and field name
pub fn find_field_usages_in_parse(
    parse: &graphql_syntax::Parse,
    type_name: &str,
    field_name: &str,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        let line_index = graphql_syntax::LineIndex::new(doc.source);
        let ranges = find_field_usages_in_tree(doc.tree, type_name, field_name, schema_types);
        for (start, end) in ranges {
            let range = offset_range_to_range(&line_index, start, end);
            results.push(adjust_range_for_line_offset(range, doc.line_offset));
        }
    }

    results
}

/// Find all directive usages in a parsed file by scanning all definitions
pub fn find_directive_usages_in_parse(
    parse: &graphql_syntax::Parse,
    directive_name: &str,
) -> Vec<Range> {
    let mut results = Vec::new();

    for doc in parse.documents() {
        let line_index = graphql_syntax::LineIndex::new(doc.source);
        let ranges = find_directive_usages_in_tree(doc.tree, directive_name);
        for (start, end) in ranges {
            let range = offset_range_to_range(&line_index, start, end);
            results.push(adjust_range_for_line_offset(range, doc.line_offset));
        }
    }

    results
}

/// Find a directive definition's name range in a parsed file
pub fn find_directive_definition_in_parse(
    parse: &graphql_syntax::Parse,
    directive_name: &str,
) -> Option<Range> {
    use apollo_parser::cst::{CstNode, Definition};

    for doc in parse.documents() {
        for definition in doc.tree.document().definitions() {
            if let Definition::DirectiveDefinition(dir_def) = definition {
                if let Some(name) = dir_def.name() {
                    if name.text() == directive_name {
                        let range = name.syntax().text_range();
                        let start: usize = range.start().into();
                        let end: usize = range.end().into();
                        let line_index = graphql_syntax::LineIndex::new(doc.source);
                        let pos_range = offset_range_to_range(&line_index, start, end);
                        return Some(adjust_range_for_line_offset(pos_range, doc.line_offset));
                    }
                }
            }
        }
    }

    None
}

/// Find all usages of a directive by name in a single syntax tree.
/// Returns `(start_offset, end_offset)` pairs for each directive name occurrence.
fn find_directive_usages_in_tree(
    tree: &apollo_parser::SyntaxTree,
    target_directive: &str,
) -> Vec<(usize, usize)> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn collect_from_directives(
        directives: &apollo_parser::cst::Directives,
        target: &str,
        results: &mut Vec<(usize, usize)>,
    ) {
        for directive in directives.directives() {
            if let Some(name) = directive.name() {
                if name.text() == target {
                    let range = name.syntax().text_range();
                    results.push((range.start().into(), range.end().into()));
                }
            }
        }
    }

    fn collect_from_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        target: &str,
        results: &mut Vec<(usize, usize)>,
    ) {
        for selection in selection_set.selections() {
            match selection {
                Selection::Field(field) => {
                    if let Some(directives) = field.directives() {
                        collect_from_directives(&directives, target, results);
                    }
                    if let Some(nested) = field.selection_set() {
                        collect_from_selection_set(&nested, target, results);
                    }
                }
                Selection::InlineFragment(inline_frag) => {
                    if let Some(directives) = inline_frag.directives() {
                        collect_from_directives(&directives, target, results);
                    }
                    if let Some(nested) = inline_frag.selection_set() {
                        collect_from_selection_set(&nested, target, results);
                    }
                }
                Selection::FragmentSpread(frag_spread) => {
                    if let Some(directives) = frag_spread.directives() {
                        collect_from_directives(&directives, target, results);
                    }
                }
            }
        }
    }

    fn collect_from_fields_definition(
        fields: &apollo_parser::cst::FieldsDefinition,
        target: &str,
        results: &mut Vec<(usize, usize)>,
    ) {
        for field in fields.field_definitions() {
            if let Some(directives) = field.directives() {
                collect_from_directives(&directives, target, results);
            }
            if let Some(args) = field.arguments_definition() {
                for arg in args.input_value_definitions() {
                    if let Some(directives) = arg.directives() {
                        collect_from_directives(&directives, target, results);
                    }
                }
            }
        }
    }

    fn collect_from_input_fields_definition(
        fields: &apollo_parser::cst::InputFieldsDefinition,
        target: &str,
        results: &mut Vec<(usize, usize)>,
    ) {
        for field in fields.input_value_definitions() {
            if let Some(directives) = field.directives() {
                collect_from_directives(&directives, target, results);
            }
        }
    }

    fn collect_from_enum_values_definition(
        values: &apollo_parser::cst::EnumValuesDefinition,
        target: &str,
        results: &mut Vec<(usize, usize)>,
    ) {
        for value in values.enum_value_definitions() {
            if let Some(directives) = value.directives() {
                collect_from_directives(&directives, target, results);
            }
        }
    }

    let mut results = Vec::new();
    let doc = tree.document();

    for definition in doc.definitions() {
        match &definition {
            Definition::OperationDefinition(op) => {
                if let Some(directives) = op.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(selection_set) = op.selection_set() {
                    collect_from_selection_set(&selection_set, target_directive, &mut results);
                }
            }
            Definition::FragmentDefinition(frag) => {
                if let Some(directives) = frag.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(selection_set) = frag.selection_set() {
                    collect_from_selection_set(&selection_set, target_directive, &mut results);
                }
            }
            Definition::ObjectTypeDefinition(obj) => {
                if let Some(directives) = obj.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(fields) = obj.fields_definition() {
                    collect_from_fields_definition(&fields, target_directive, &mut results);
                }
            }
            Definition::InterfaceTypeDefinition(iface) => {
                if let Some(directives) = iface.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(fields) = iface.fields_definition() {
                    collect_from_fields_definition(&fields, target_directive, &mut results);
                }
            }
            Definition::UnionTypeDefinition(union_def) => {
                if let Some(directives) = union_def.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
            }
            Definition::EnumTypeDefinition(enum_def) => {
                if let Some(directives) = enum_def.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(values) = enum_def.enum_values_definition() {
                    collect_from_enum_values_definition(&values, target_directive, &mut results);
                }
            }
            Definition::ScalarTypeDefinition(scalar) => {
                if let Some(directives) = scalar.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
            }
            Definition::InputObjectTypeDefinition(input) => {
                if let Some(directives) = input.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(fields) = input.input_fields_definition() {
                    collect_from_input_fields_definition(&fields, target_directive, &mut results);
                }
            }
            Definition::SchemaDefinition(schema) => {
                if let Some(directives) = schema.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
            }
            Definition::ObjectTypeExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(fields) = ext.fields_definition() {
                    collect_from_fields_definition(&fields, target_directive, &mut results);
                }
            }
            Definition::InterfaceTypeExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(fields) = ext.fields_definition() {
                    collect_from_fields_definition(&fields, target_directive, &mut results);
                }
            }
            Definition::UnionTypeExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
            }
            Definition::EnumTypeExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(values) = ext.enum_values_definition() {
                    collect_from_enum_values_definition(&values, target_directive, &mut results);
                }
            }
            Definition::ScalarTypeExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
            }
            Definition::InputObjectTypeExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
                if let Some(fields) = ext.input_fields_definition() {
                    collect_from_input_fields_definition(&fields, target_directive, &mut results);
                }
            }
            Definition::SchemaExtension(ext) => {
                if let Some(directives) = ext.directives() {
                    collect_from_directives(&directives, target_directive, &mut results);
                }
            }
            Definition::DirectiveDefinition(_) => {}
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

/// Context about a field argument at a cursor position
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentContext {
    /// The field name this argument belongs to
    pub field_name: String,
    /// The argument name, if the cursor is inside a specific argument's value
    pub argument_name: Option<String>,
}

/// Find the argument context at a given offset.
///
/// Returns the field name and (optionally) which argument's value position the cursor is in.
/// This is used for enum value completions and input object field completions.
///
/// We use a two-pronged approach:
/// 1. Check CST argument nodes for cursor position
/// 2. Scan the source text before the cursor to detect `argName:` patterns
///    (handles cases where the parser doesn't produce a value node)
pub fn find_argument_context_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<ArgumentContext> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn check_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        byte_offset: usize,
        source: &str,
    ) -> Option<ArgumentContext> {
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
                            let field_name = field.name()?.text().to_string();

                            // Check if cursor is inside a specific argument's value
                            // by examining the CST argument nodes
                            for arg in args.arguments() {
                                let arg_range = arg.syntax().text_range();
                                let arg_start: usize = arg_range.start().into();
                                let arg_end: usize = arg_range.end().into();
                                if byte_offset >= arg_start && byte_offset <= arg_end {
                                    if let Some(name) = arg.name() {
                                        let name_end: usize =
                                            name.syntax().text_range().end().into();
                                        if byte_offset > name_end {
                                            return Some(ArgumentContext {
                                                field_name,
                                                argument_name: Some(name.text().to_string()),
                                            });
                                        }
                                    }
                                }
                            }

                            // Fallback: scan text before cursor for `argName:` pattern
                            // This handles cases like `field(status: |)` where the parser
                            // may not include the cursor position in the argument node range
                            if let Some(arg_name) =
                                find_preceding_arg_name(source, byte_offset, args_start)
                            {
                                return Some(ArgumentContext {
                                    field_name,
                                    argument_name: Some(arg_name),
                                });
                            }

                            return Some(ArgumentContext {
                                field_name,
                                argument_name: None,
                            });
                        }
                    }

                    if let Some(nested) = field.selection_set() {
                        if let Some(ctx) = check_selection_set(&nested, byte_offset, source) {
                            return Some(ctx);
                        }
                    }
                }
            }
        }
        None
    }

    let source = tree.document().syntax().to_string();
    let doc = tree.document();
    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    if let Some(ctx) = check_selection_set(&selection_set, byte_offset, &source) {
                        return Some(ctx);
                    }
                }
            }
            Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    if let Some(ctx) = check_selection_set(&selection_set, byte_offset, &source) {
                        return Some(ctx);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Scan backwards from cursor to find an `argName:` pattern.
/// Returns the argument name if found.
///
/// Handles patterns like:
/// - `argName: |` (cursor right after colon)
/// - `argName: {|` (cursor inside object value)
/// - `argName: { field: value, |` (cursor inside nested object)
fn find_preceding_arg_name(
    source: &str,
    cursor_offset: usize,
    args_start: usize,
) -> Option<String> {
    let before_cursor = source.get(args_start..cursor_offset)?;
    let trimmed = before_cursor.trim_end();

    // Direct case: text ends with `:` (cursor right after colon + optional whitespace)
    if let Some(before_colon) = trimmed.strip_suffix(':') {
        return extract_arg_name(before_colon);
    }

    // Object value case: text ends with `{` after `argName:` pattern
    // Scan backwards, tracking brace depth to find the matching `argName:`
    let mut depth = 0i32;
    let bytes = before_cursor.as_bytes();
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    // Found the opening brace at our level, look for `argName:` before it
                    let before_brace = before_cursor[..i].trim_end();
                    if let Some(before_colon) = before_brace.strip_suffix(':') {
                        return extract_arg_name(before_colon);
                    }
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    None
}

/// Extract an argument name from the text before a colon.
fn extract_arg_name(before_colon: &str) -> Option<String> {
    let before_colon = before_colon.trim_end();
    let arg_name = before_colon.rsplit([',', '(']).next()?.trim();
    if !arg_name.is_empty() && arg_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some(arg_name.to_string());
    }
    None
}

/// Context about a directive argument at a cursor position
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveArgumentContext {
    /// The directive name this argument belongs to
    pub directive_name: String,
    /// The argument name, if the cursor is inside a specific argument's value
    pub argument_name: Option<String>,
}

/// Find the directive argument context at a given offset.
///
/// Walks through all definitions and their directives (including directives on
/// fields, inline fragments, and fragment spreads within selection sets) to check
/// if the cursor is inside a directive's arguments list.
pub fn find_directive_argument_context_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<DirectiveArgumentContext> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn check_directives(
        directives: &apollo_parser::cst::Directives,
        byte_offset: usize,
        source: &str,
    ) -> Option<DirectiveArgumentContext> {
        for directive in directives.directives() {
            if let Some(args) = directive.arguments() {
                let args_range = args.syntax().text_range();
                let args_start: usize = args_range.start().into();
                let args_end: usize = args_range.end().into();
                if byte_offset >= args_start && byte_offset <= args_end {
                    let directive_name = directive.name()?.text().to_string();

                    // Check CST argument nodes for value position
                    for arg in args.arguments() {
                        let arg_range = arg.syntax().text_range();
                        let arg_start: usize = arg_range.start().into();
                        let arg_end: usize = arg_range.end().into();
                        if byte_offset >= arg_start && byte_offset <= arg_end {
                            if let Some(name) = arg.name() {
                                let name_end: usize = name.syntax().text_range().end().into();
                                if byte_offset > name_end {
                                    return Some(DirectiveArgumentContext {
                                        directive_name,
                                        argument_name: Some(name.text().to_string()),
                                    });
                                }
                            }
                        }
                    }

                    // Fallback: scan text before cursor for `argName:` pattern
                    if let Some(arg_name) = find_preceding_arg_name(source, byte_offset, args_start)
                    {
                        return Some(DirectiveArgumentContext {
                            directive_name,
                            argument_name: Some(arg_name),
                        });
                    }

                    return Some(DirectiveArgumentContext {
                        directive_name,
                        argument_name: None,
                    });
                }
            }
        }
        None
    }

    fn check_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        byte_offset: usize,
        source: &str,
    ) -> Option<DirectiveArgumentContext> {
        for selection in selection_set.selections() {
            match selection {
                Selection::Field(field) => {
                    if let Some(directives) = field.directives() {
                        if let Some(ctx) = check_directives(&directives, byte_offset, source) {
                            return Some(ctx);
                        }
                    }
                    if let Some(nested) = field.selection_set() {
                        if let Some(ctx) = check_selection_set(&nested, byte_offset, source) {
                            return Some(ctx);
                        }
                    }
                }
                Selection::InlineFragment(inline_frag) => {
                    if let Some(directives) = inline_frag.directives() {
                        if let Some(ctx) = check_directives(&directives, byte_offset, source) {
                            return Some(ctx);
                        }
                    }
                    if let Some(nested) = inline_frag.selection_set() {
                        if let Some(ctx) = check_selection_set(&nested, byte_offset, source) {
                            return Some(ctx);
                        }
                    }
                }
                Selection::FragmentSpread(frag_spread) => {
                    if let Some(directives) = frag_spread.directives() {
                        if let Some(ctx) = check_directives(&directives, byte_offset, source) {
                            return Some(ctx);
                        }
                    }
                }
            }
        }
        None
    }

    let source = tree.document().syntax().to_string();
    let doc = tree.document();
    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                if let Some(directives) = op.directives() {
                    if let Some(ctx) = check_directives(&directives, byte_offset, &source) {
                        return Some(ctx);
                    }
                }
                if let Some(selection_set) = op.selection_set() {
                    if let Some(ctx) = check_selection_set(&selection_set, byte_offset, &source) {
                        return Some(ctx);
                    }
                }
            }
            Definition::FragmentDefinition(frag) => {
                if let Some(directives) = frag.directives() {
                    if let Some(ctx) = check_directives(&directives, byte_offset, &source) {
                        return Some(ctx);
                    }
                }
                if let Some(selection_set) = frag.selection_set() {
                    if let Some(ctx) = check_selection_set(&selection_set, byte_offset, &source) {
                        return Some(ctx);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Find variable definitions from the operation that contains the given offset.
///
/// Returns variable names and their types for completions.
pub fn find_operation_variables_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<Vec<(String, String)>> {
    use apollo_parser::cst::{CstNode, Definition};

    let doc = tree.document();
    for definition in doc.definitions() {
        if let Definition::OperationDefinition(op) = definition {
            let range = op.syntax().text_range();
            let start: usize = range.start().into();
            let end: usize = range.end().into();
            if byte_offset >= start && byte_offset <= end {
                let mut variables = Vec::new();
                if let Some(var_defs) = op.variable_definitions() {
                    for var_def in var_defs.variable_definitions() {
                        if let Some(variable) = var_def.variable() {
                            if let Some(name) = variable.name() {
                                let type_str = var_def
                                    .ty()
                                    .map(|t| t.syntax().to_string())
                                    .unwrap_or_default();
                                variables.push((name.text().to_string(), type_str));
                            }
                        }
                    }
                }
                return Some(variables);
            }
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
    fn test_find_argument_context_inside_object_value() {
        let source = r#"mutation CreateUser {
    createUser(input: { name: "test", }) {
        id
    }
}"#;
        let parser = apollo_parser::Parser::new(source);
        let tree = parser.parse();
        // Cursor between ", " and "}" inside the object value
        let cursor_pos = source.find(", }").unwrap() + 2;

        let ctx = find_argument_context_at_offset(&tree, cursor_pos);
        assert!(
            ctx.is_some(),
            "Should find argument context inside object value"
        );
        let ctx = ctx.unwrap();
        assert_eq!(ctx.field_name, "createUser");
        assert_eq!(ctx.argument_name.as_deref(), Some("input"));
    }

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
    fn test_position_to_offset_with_emoji() {
        // 🚀 is 4 bytes in UTF-8, 2 code units in UTF-16
        let text = "# \u{1F680} Launch\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(text);

        // Position(0, 5) in UTF-16: '#'(1) + ' '(1) + 🚀(2) + ' '(1) = 5
        // Should map to byte offset 7: '#'(1) + ' '(1) + 🚀(4) + ' '(1) = 7
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 5)),
            Some(7),
            "UTF-16 offset 5 should map to byte offset 7 (emoji is 4 bytes but 2 UTF-16 units)"
        );
    }

    #[test]
    fn test_position_to_offset_with_cjk() {
        // CJK characters: 3 bytes in UTF-8, 1 code unit in UTF-16
        let text = "# \u{7528}\u{6237}\u{67E5}\u{8BE2}\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(text);

        // Position(0, 3) in UTF-16: '#'(1) + ' '(1) + 用(1) = 3
        // Should map to byte offset 5: '#'(1) + ' '(1) + 用(3) = 5
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 3)),
            Some(5),
            "UTF-16 offset 3 should map to byte offset 5 (CJK is 3 bytes but 1 UTF-16 unit)"
        );
    }

    #[test]
    fn test_offset_to_position_with_emoji() {
        let text = "# \u{1F680} Launch\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(text);

        // Byte offset 7 = 'L' in "Launch"
        // UTF-16 position: '#'(1) + ' '(1) + 🚀(2) + ' '(1) = 5
        assert_eq!(
            offset_to_position(&line_index, 7),
            Position::new(0, 5),
            "Byte offset 7 should map to UTF-16 column 5 (emoji is 4 bytes but 2 UTF-16 units)"
        );
    }

    #[test]
    fn test_offset_to_position_with_cjk() {
        let text = "# \u{7528}\u{6237}\u{67E5}\u{8BE2}\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(text);

        // Byte offset 5 = start of 户 (second CJK char)
        // UTF-16 position: '#'(1) + ' '(1) + 用(1) = 3
        assert_eq!(
            offset_to_position(&line_index, 5),
            Position::new(0, 3),
            "Byte offset 5 should map to UTF-16 column 3 (CJK is 3 bytes but 1 UTF-16 unit)"
        );
    }

    #[test]
    fn test_offset_range_to_range_with_multibyte() {
        let text = "# \u{1F680} Launch\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(text);

        // "Launch" = bytes 7..13
        // UTF-16: start at (0, 5), end at (0, 11)
        let range = offset_range_to_range(&line_index, 7, 13);
        assert_eq!(range.start, Position::new(0, 5));
        assert_eq!(range.end, Position::new(0, 11));
    }

    #[test]
    fn test_position_to_offset_ascii_unchanged() {
        // ASCII-only text should work identically (byte offset == UTF-16 offset)
        let text = "query {\n  user {\n    name\n  }\n}";
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
            position_to_offset(&line_index, Position::new(1, 2)),
            Some(10)
        );
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
