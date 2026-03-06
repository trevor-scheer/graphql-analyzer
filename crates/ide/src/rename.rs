//! Rename symbol feature implementation.
//!
//! Supports renaming:
//! - Fragment names (project-wide, across all spreads and the definition)
//! - Operation names (file-local, definition only)
//! - Variable names (file-local, definition and all references within the operation)

use std::collections::HashMap;

use apollo_parser::cst::CstNode;

use crate::helpers::{find_block_for_position, offset_range_to_range, position_to_offset};
use crate::symbol::{find_symbol_at_offset, Symbol};
use crate::types::{FilePath, Location, Position, Range, RenameResult, TextEdit};
use crate::FileRegistry;

/// Check if the symbol at a position can be renamed, returning its range.
pub fn prepare_rename(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    file: &FilePath,
    position: Position,
) -> Option<Range> {
    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;
        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;
        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let (block_context, adjusted_position) = find_block_for_position(&parse, position)?;
    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;
    let symbol = find_symbol_at_offset(block_context.tree, offset)?;

    match symbol {
        Symbol::FragmentSpread { ref name }
        | Symbol::OperationName { ref name }
        | Symbol::VariableReference { ref name } => {
            let (start, end) = find_name_range_at_offset(block_context.tree, offset, name)?;
            let mut range = offset_range_to_range(&block_line_index, start, end);
            range.start.line += block_context.line_offset;
            range.end.line += block_context.line_offset;
            Some(range)
        }
        // Schema symbols cannot be renamed through document operations
        Symbol::TypeName { .. } | Symbol::FieldName { .. } | Symbol::ArgumentName { .. } => None,
    }
}

/// Rename the symbol at a position to a new name.
pub fn rename(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
    new_name: &str,
) -> Option<RenameResult> {
    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;
        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;
        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let (block_context, adjusted_position) = find_block_for_position(&parse, position)?;
    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;
    let symbol = find_symbol_at_offset(block_context.tree, offset)?;

    match symbol {
        Symbol::FragmentSpread { name } => {
            rename_fragment(db, registry, project_files, &name, new_name)
        }
        Symbol::OperationName { name } => rename_operation(db, registry, file, &name, new_name),
        Symbol::VariableReference { name } => rename_variable(db, registry, file, &name, new_name),
        Symbol::TypeName { .. } | Symbol::FieldName { .. } | Symbol::ArgumentName { .. } => None,
    }
}

/// Rename a fragment across all files in the project.
fn rename_fragment(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    old_name: &str,
    new_name: &str,
) -> Option<RenameResult> {
    // Reuse find_references to get all locations (definition + spreads)
    let locations = crate::references::find_fragment_references(
        db,
        registry,
        project_files,
        old_name,
        true, // include declaration
    );

    if locations.is_empty() {
        return None;
    }

    Some(locations_to_rename_result(&locations, new_name))
}

/// Rename an operation name within a single file.
fn rename_operation(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    file: &FilePath,
    old_name: &str,
    new_name: &str,
) -> Option<RenameResult> {
    let file_id = registry.get_file_id(file)?;
    let content = registry.get_content(file_id)?;
    let metadata = registry.get_metadata(file_id)?;
    let file_path = registry.get_path(file_id)?;

    let parse = graphql_syntax::parse(db, content, metadata);

    let mut locations = Vec::new();
    for doc in parse.documents() {
        let tree = doc.tree;
        if let Some(ranges) = crate::symbol::find_operation_definition_ranges(tree, old_name) {
            let line_index = graphql_syntax::LineIndex::new(doc.source);
            let range = offset_range_to_range(&line_index, ranges.name_start, ranges.name_end);
            let mut adjusted = range;
            adjusted.start.line += doc.line_offset;
            adjusted.end.line += doc.line_offset;
            locations.push(Location::new(file_path.clone(), adjusted));
        }
    }

    if locations.is_empty() {
        return None;
    }

    Some(locations_to_rename_result(&locations, new_name))
}

/// Rename a variable within the containing operation (definition + all usages).
fn rename_variable(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    file: &FilePath,
    old_name: &str,
    new_name: &str,
) -> Option<RenameResult> {
    use apollo_parser::cst;

    let file_id = registry.get_file_id(file)?;
    let content = registry.get_content(file_id)?;
    let metadata = registry.get_metadata(file_id)?;
    let file_path = registry.get_path(file_id)?;

    let parse = graphql_syntax::parse(db, content, metadata);

    let mut locations = Vec::new();
    for doc in parse.documents() {
        let tree = doc.tree;
        let line_index = graphql_syntax::LineIndex::new(doc.source);
        let document = tree.document();

        for definition in document.definitions() {
            if let cst::Definition::OperationDefinition(op) = definition {
                // Check if this operation defines the variable
                let has_var = op.variable_definitions().is_some_and(|var_defs| {
                    var_defs.variable_definitions().any(|vd| {
                        vd.variable()
                            .and_then(|v| v.name())
                            .is_some_and(|n| n.text() == old_name)
                    })
                });

                if !has_var {
                    continue;
                }

                // Collect variable definition location
                if let Some(var_defs) = op.variable_definitions() {
                    for var_def in var_defs.variable_definitions() {
                        if let Some(variable) = var_def.variable() {
                            if let Some(name) = variable.name() {
                                if name.text() == old_name {
                                    let range = name.syntax().text_range();
                                    let start: usize = range.start().into();
                                    let end: usize = range.end().into();
                                    let mut r = offset_range_to_range(&line_index, start, end);
                                    r.start.line += doc.line_offset;
                                    r.end.line += doc.line_offset;
                                    locations.push(Location::new(file_path.clone(), r));
                                }
                            }
                        }
                    }
                }

                // Collect all variable references in the operation body
                if let Some(selection_set) = op.selection_set() {
                    collect_variable_references_in_selection_set(
                        &selection_set,
                        old_name,
                        &line_index,
                        doc.line_offset,
                        &file_path,
                        &mut locations,
                    );
                }
            }
        }
    }

    if locations.is_empty() {
        return None;
    }

    Some(locations_to_rename_result(&locations, new_name))
}

fn collect_variable_references_in_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
    var_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    file_path: &FilePath,
    locations: &mut Vec<Location>,
) {
    use apollo_parser::cst;

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(arguments) = field.arguments() {
                    collect_variable_references_in_arguments(
                        &arguments,
                        var_name,
                        line_index,
                        line_offset,
                        file_path,
                        locations,
                    );
                }
                if let Some(nested) = field.selection_set() {
                    collect_variable_references_in_selection_set(
                        &nested,
                        var_name,
                        line_index,
                        line_offset,
                        file_path,
                        locations,
                    );
                }
            }
            cst::Selection::InlineFragment(inline_frag) => {
                if let Some(directives) = inline_frag.directives() {
                    collect_variable_references_in_directives(
                        &directives,
                        var_name,
                        line_index,
                        line_offset,
                        file_path,
                        locations,
                    );
                }
                if let Some(nested) = inline_frag.selection_set() {
                    collect_variable_references_in_selection_set(
                        &nested,
                        var_name,
                        line_index,
                        line_offset,
                        file_path,
                        locations,
                    );
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(directives) = spread.directives() {
                    collect_variable_references_in_directives(
                        &directives,
                        var_name,
                        line_index,
                        line_offset,
                        file_path,
                        locations,
                    );
                }
            }
        }
    }
}

fn collect_variable_references_in_arguments(
    arguments: &apollo_parser::cst::Arguments,
    var_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    file_path: &FilePath,
    locations: &mut Vec<Location>,
) {
    for arg in arguments.arguments() {
        if let Some(value) = arg.value() {
            collect_variable_references_in_value(
                &value,
                var_name,
                line_index,
                line_offset,
                file_path,
                locations,
            );
        }
    }
}

fn collect_variable_references_in_directives(
    directives: &apollo_parser::cst::Directives,
    var_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    file_path: &FilePath,
    locations: &mut Vec<Location>,
) {
    for directive in directives.directives() {
        if let Some(arguments) = directive.arguments() {
            collect_variable_references_in_arguments(
                &arguments,
                var_name,
                line_index,
                line_offset,
                file_path,
                locations,
            );
        }
    }
}

fn collect_variable_references_in_value(
    value: &apollo_parser::cst::Value,
    var_name: &str,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    file_path: &FilePath,
    locations: &mut Vec<Location>,
) {
    use apollo_parser::cst;

    match value {
        cst::Value::Variable(var) => {
            if let Some(name) = var.name() {
                if name.text() == var_name {
                    let range = name.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    let mut r = offset_range_to_range(line_index, start, end);
                    r.start.line += line_offset;
                    r.end.line += line_offset;
                    locations.push(Location::new(file_path.clone(), r));
                }
            }
        }
        cst::Value::ListValue(list) => {
            for val in list.values() {
                collect_variable_references_in_value(
                    &val,
                    var_name,
                    line_index,
                    line_offset,
                    file_path,
                    locations,
                );
            }
        }
        cst::Value::ObjectValue(obj) => {
            for field in obj.object_fields() {
                if let Some(val) = field.value() {
                    collect_variable_references_in_value(
                        &val,
                        var_name,
                        line_index,
                        line_offset,
                        file_path,
                        locations,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Convert a list of reference locations into a `RenameResult` with text edits.
fn locations_to_rename_result(locations: &[Location], new_name: &str) -> RenameResult {
    let mut changes: HashMap<FilePath, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        changes
            .entry(loc.file.clone())
            .or_default()
            .push(TextEdit::new(loc.range, new_name));
    }
    RenameResult::new(changes)
}

/// Find the byte range of a name at a given offset in the CST.
///
/// Walks descendant tokens to find one matching the expected name that
/// contains the byte offset, avoiding dependency on rowan's `token_at_offset`.
fn find_name_range_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
    expected_name: &str,
) -> Option<(usize, usize)> {
    use apollo_parser::cst::CstNode;

    let document = tree.document();
    let root = document.syntax();
    for element in root.descendants_with_tokens() {
        if let apollo_parser::SyntaxElement::Token(token) = element {
            if token.kind() == apollo_parser::SyntaxKind::IDENT && token.text() == expected_name {
                let range = token.text_range();
                let start: usize = range.start().into();
                let end: usize = range.end().into();
                if byte_offset >= start && byte_offset < end {
                    return Some((start, end));
                }
            }
        }
    }
    None
}
