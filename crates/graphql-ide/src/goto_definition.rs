//! Go to Definition feature implementation.
//!
//! This module provides IDE "go to definition" functionality for:
//! - Field definitions in schema types
//! - Fragment definitions
//! - Type definitions
//! - Variable definitions
//! - Argument definitions
//! - Operation definitions

use crate::helpers::{
    adjust_range_for_line_offset, find_argument_definition_in_tree, find_block_for_position,
    find_field_name_at_offset, find_fragment_definition_in_parse,
    find_operation_definition_in_tree, find_type_definition_in_parse,
    find_variable_definition_in_tree, offset_range_to_range, position_to_offset,
};
use crate::symbol::{
    find_field_definition_full_range, find_parent_type_at_offset, find_symbol_at_offset, Symbol,
};
use crate::types::{FilePath, Location, Position};
use crate::FileRegistry;

/// Get goto definition locations for the symbol at a position.
///
/// Returns the definition location(s) for types, fields, fragments, etc.
#[allow(clippy::too_many_lines)]
pub fn goto_definition(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
) -> Option<Vec<Location>> {
    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;

        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;

        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let line_index = graphql_syntax::line_index(db, content);

    let metadata_line_offset = metadata.line_offset(db);
    let (block_context, adjusted_position) =
        find_block_for_position(&parse, position, metadata_line_offset)?;

    tracing::debug!(
        "Goto definition: original position {:?}, block line_offset {}, adjusted position {:?}",
        position,
        block_context.line_offset,
        adjusted_position
    );

    let offset = if let Some(block_source) = block_context.block_source {
        let block_line_index = graphql_syntax::LineIndex::new(block_source);
        position_to_offset(&block_line_index, adjusted_position)?
    } else {
        position_to_offset(&line_index, adjusted_position)?
    };

    let symbol = find_symbol_at_offset(block_context.tree, offset)?;
    let project_files = project_files?;

    match symbol {
        Symbol::FieldName { name } => goto_field_definition(
            db,
            registry,
            project_files,
            block_context.tree,
            offset,
            &name,
        ),
        Symbol::FragmentSpread { name } => {
            goto_fragment_definition(db, registry, project_files, &name)
        }
        Symbol::TypeName { name } => goto_type_definition(db, registry, project_files, &name),
        Symbol::VariableReference { name } => {
            let range = if let Some(block_source) = block_context.block_source {
                let block_line_index = graphql_syntax::LineIndex::new(block_source);
                find_variable_definition_in_tree(
                    block_context.tree,
                    &name,
                    &block_line_index,
                    block_context.line_offset,
                )
            } else {
                let file_line_index = graphql_syntax::line_index(db, content);
                find_variable_definition_in_tree(
                    block_context.tree,
                    &name,
                    &file_line_index,
                    block_context.line_offset,
                )
            };

            if let Some(range) = range {
                let file_id = registry.get_file_id(file)?;
                let file_path = registry.get_path(file_id)?;
                return Some(vec![Location::new(file_path, range)]);
            }
            None
        }
        Symbol::ArgumentName { name } => goto_argument_definition(
            db,
            registry,
            project_files,
            block_context.tree,
            offset,
            &name,
        ),
        Symbol::OperationName { name } => {
            let range = if let Some(block_source) = block_context.block_source {
                let block_line_index = graphql_syntax::LineIndex::new(block_source);
                find_operation_definition_in_tree(
                    block_context.tree,
                    &name,
                    &block_line_index,
                    block_context.line_offset,
                )
            } else {
                let file_line_index = graphql_syntax::line_index(db, content);
                find_operation_definition_in_tree(
                    block_context.tree,
                    &name,
                    &file_line_index,
                    block_context.line_offset,
                )
            };

            if let Some(range) = range {
                let file_id = registry.get_file_id(file)?;
                let file_path = registry.get_path(file_id)?;
                return Some(vec![Location::new(file_path, range)]);
            }
            None
        }
    }
}

fn goto_field_definition(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: graphql_db::ProjectFiles,
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
    field_name: &str,
) -> Option<Vec<Location>> {
    let parent_context = find_parent_type_at_offset(tree, offset)?;
    let schema_types = graphql_hir::schema_types(db, project_files);

    let parent_type_name = crate::symbol::walk_type_stack_to_offset(
        tree,
        schema_types,
        offset,
        &parent_context.root_type,
    )?;

    tracing::debug!(
        "Field '{}' - resolved parent type '{}' (root: {})",
        field_name,
        parent_type_name,
        parent_context.root_type
    );

    schema_types.get(parent_type_name.as_str())?;

    let schema_file_ids = project_files.schema_file_ids(db).ids(db);

    for file_id in schema_file_ids.iter() {
        let Some(schema_content) = registry.get_content(*file_id) else {
            continue;
        };
        let Some(schema_metadata) = registry.get_metadata(*file_id) else {
            continue;
        };
        let Some(file_path) = registry.get_path(*file_id) else {
            continue;
        };

        let schema_parse = graphql_syntax::parse(db, schema_content, schema_metadata);
        let schema_line_index = graphql_syntax::line_index(db, schema_content);
        let schema_line_offset = schema_metadata.line_offset(db);

        if schema_parse.blocks.is_empty() {
            if let Some(ranges) =
                find_field_definition_full_range(&schema_parse.tree, &parent_type_name, field_name)
            {
                let range =
                    offset_range_to_range(&schema_line_index, ranges.name_start, ranges.name_end);
                let adjusted_range = adjust_range_for_line_offset(range, schema_line_offset);
                return Some(vec![Location::new(file_path, adjusted_range)]);
            }
        } else {
            for block in &schema_parse.blocks {
                if let Some(ranges) =
                    find_field_definition_full_range(&block.tree, &parent_type_name, field_name)
                {
                    let block_line_index = graphql_syntax::LineIndex::new(&block.source);
                    let range = offset_range_to_range(
                        &block_line_index,
                        ranges.name_start,
                        ranges.name_end,
                    );
                    #[allow(clippy::cast_possible_truncation)]
                    let block_line_offset = block.line as u32;
                    let adjusted_range = adjust_range_for_line_offset(range, block_line_offset);
                    return Some(vec![Location::new(file_path, adjusted_range)]);
                }
            }
        }
    }

    None
}

fn goto_fragment_definition(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: graphql_db::ProjectFiles,
    fragment_name: &str,
) -> Option<Vec<Location>> {
    let fragments = graphql_hir::all_fragments(db, project_files);

    tracing::debug!(
        "Looking for fragment '{}', available fragments: {:?}",
        fragment_name,
        fragments.keys().collect::<Vec<_>>()
    );

    let fragment = fragments.get(fragment_name)?;

    tracing::debug!(
        "Looking up path for fragment '{}' with FileId {:?}",
        fragment_name,
        fragment.file_id
    );

    let file_path = registry.get_path(fragment.file_id)?;
    let def_content = registry.get_content(fragment.file_id)?;
    let def_metadata = registry.get_metadata(fragment.file_id)?;

    let def_parse = graphql_syntax::parse(db, def_content, def_metadata);
    let def_line_offset = def_metadata.line_offset(db);

    let range = find_fragment_definition_in_parse(
        &def_parse,
        fragment_name,
        def_content,
        db,
        def_line_offset,
    )?;

    Some(vec![Location::new(file_path, range)])
}

fn goto_type_definition(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: graphql_db::ProjectFiles,
    type_name: &str,
) -> Option<Vec<Location>> {
    let schema_ids = project_files.schema_file_ids(db).ids(db);

    for file_id in schema_ids.iter() {
        let Some(schema_content) = registry.get_content(*file_id) else {
            continue;
        };
        let Some(schema_metadata) = registry.get_metadata(*file_id) else {
            continue;
        };
        let Some(file_path) = registry.get_path(*file_id) else {
            continue;
        };

        let schema_parse = graphql_syntax::parse(db, schema_content, schema_metadata);
        let schema_line_offset = schema_metadata.line_offset(db);

        if let Some(range) = find_type_definition_in_parse(
            &schema_parse,
            type_name,
            schema_content,
            db,
            schema_line_offset,
        ) {
            return Some(vec![Location::new(file_path, range)]);
        }
    }

    None
}

fn goto_argument_definition(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: graphql_db::ProjectFiles,
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
    arg_name: &str,
) -> Option<Vec<Location>> {
    let parent_context = find_parent_type_at_offset(tree, offset)?;
    let schema_types = graphql_hir::schema_types(db, project_files);

    let field_name = find_field_name_at_offset(tree, offset)?;

    let parent_type_name = crate::symbol::walk_type_stack_to_offset(
        tree,
        schema_types,
        offset,
        &parent_context.root_type,
    )?;

    let schema_file_ids = project_files.schema_file_ids(db).ids(db);

    for file_id in schema_file_ids.iter() {
        let Some(schema_content) = registry.get_content(*file_id) else {
            continue;
        };
        let Some(schema_metadata) = registry.get_metadata(*file_id) else {
            continue;
        };
        let Some(file_path) = registry.get_path(*file_id) else {
            continue;
        };

        let schema_parse = graphql_syntax::parse(db, schema_content, schema_metadata);
        let schema_line_index = graphql_syntax::line_index(db, schema_content);
        let schema_line_offset = schema_metadata.line_offset(db);

        if let Some(range) = find_argument_definition_in_tree(
            &schema_parse.tree,
            &parent_type_name,
            &field_name,
            arg_name,
            &schema_line_index,
            schema_line_offset,
        ) {
            return Some(vec![Location::new(file_path, range)]);
        }
    }

    None
}
