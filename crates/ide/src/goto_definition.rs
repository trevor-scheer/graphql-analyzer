//! Goto definition feature implementation.
//!
//! This module provides IDE goto definition functionality for:
//! - Field definitions in schema
//! - Fragment definitions
//! - Type definitions
//! - Variable definitions
//! - Argument definitions
//! - Operation definitions

use crate::helpers::{
    find_fragment_definition_in_parse, find_operation_definition_in_tree,
    find_variable_definition_in_tree, offset_range_to_range, position_to_offset,
};
use crate::symbol::{find_parent_type_at_offset, find_symbol_at_offset, Symbol};
use crate::types::{FilePath, Location, Position};
use crate::{helpers::find_block_for_position, symbol, FileRegistry};

/// Get goto definition locations for the symbol at a position.
///
/// Returns the definition location(s) for types, fields, fragments, etc.
pub fn goto_definition(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
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

    let (block_context, adjusted_position) = find_block_for_position(&parse, position)?;

    tracing::debug!(
        "Goto definition: original position {:?}, block line_offset {}, adjusted position {:?}",
        position,
        block_context.line_offset,
        adjusted_position
    );

    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;

    let symbol = find_symbol_at_offset(block_context.tree, offset)?;

    let project_files = project_files?;

    match symbol {
        Symbol::FieldName { name } => {
            let schema_types = graphql_hir::schema_types(db, project_files);

            let parent_type_name =
                if let Some(parent_ctx) = find_parent_type_at_offset(block_context.tree, offset) {
                    symbol::walk_type_stack_to_offset(
                        block_context.tree,
                        schema_types,
                        offset,
                        &parent_ctx.root_type,
                    )?
                } else {
                    symbol::find_schema_field_parent_type(block_context.tree, offset)?
                };

            tracing::debug!(
                "Field '{}' - resolved parent type '{}'",
                name,
                parent_type_name
            );

            let type_def = schema_types.get(parent_type_name.as_str())?;
            let field = type_def.fields.iter().find(|f| f.name.as_ref() == name)?;

            let file_path = registry.get_path(field.file_id)?;
            let content = registry.get_content(field.file_id)?;
            let line_index = graphql_syntax::line_index(db, content);
            let start: usize = field.name_range.start().into();
            let end: usize = field.name_range.end().into();
            let range = offset_range_to_range(&line_index, start, end);

            Some(vec![Location::new(file_path, range)])
        }
        Symbol::FragmentSpread { name } => {
            let fragments = graphql_hir::all_fragments(db, project_files);

            tracing::debug!(
                "Looking for fragment '{}', available fragments: {:?}",
                name,
                fragments.keys().collect::<Vec<_>>()
            );

            let fragment = fragments.get(name.as_str())?;

            tracing::debug!(
                "Looking up path for fragment '{}' with FileId {:?}",
                name,
                fragment.file_id
            );
            let all_ids = registry.all_file_ids();
            tracing::debug!("Registry has {} files", all_ids.len());
            tracing::debug!("Registry FileIds: {:?}", all_ids);

            let Some(file_path) = registry.get_path(fragment.file_id) else {
                tracing::error!(
                    "FileId {:?} not found in registry for fragment '{}'",
                    fragment.file_id,
                    name
                );
                return None;
            };
            let def_content = registry.get_content(fragment.file_id)?;
            let def_metadata = registry.get_metadata(fragment.file_id)?;

            let def_parse = graphql_syntax::parse(db, def_content, def_metadata);

            let range = find_fragment_definition_in_parse(&def_parse, &name, def_content, db)?;

            Some(vec![Location::new(file_path, range)])
        }
        Symbol::TypeName { name } => {
            // Collect all definitions (base + extensions) across all schema files
            // using per-file HIR type defs for O(1)-per-file lookup.
            let schema_file_ids = project_files.schema_file_ids(db).ids(db);
            let mut locations = Vec::new();

            for file_id in schema_file_ids.iter() {
                let Some((file_content, file_metadata)) =
                    graphql_base_db::file_lookup(db, project_files, *file_id)
                else {
                    continue;
                };
                let type_defs =
                    graphql_hir::file_type_defs(db, *file_id, file_content, file_metadata);
                for type_def in type_defs.iter() {
                    if type_def.name.as_ref() == name {
                        if let Some(file_path) = registry.get_path(type_def.file_id) {
                            let content = registry.get_content(type_def.file_id)?;
                            let line_index = graphql_syntax::line_index(db, content);
                            let start: usize = type_def.name_range.start().into();
                            let end: usize = type_def.name_range.end().into();
                            let range = offset_range_to_range(&line_index, start, end);
                            locations.push(Location::new(file_path, range));
                        }
                    }
                }
            }

            if locations.is_empty() {
                None
            } else {
                Some(locations)
            }
        }
        Symbol::VariableReference { name } => {
            let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
            let range = find_variable_definition_in_tree(
                block_context.tree,
                &name,
                &block_line_index,
                block_context.line_offset,
            );

            if let Some(range) = range {
                let file_id = registry.get_file_id(file)?;
                let file_path = registry.get_path(file_id)?;
                return Some(vec![Location::new(file_path, range)]);
            }
            None
        }
        Symbol::ArgumentName { name } => {
            let parent_context = find_parent_type_at_offset(block_context.tree, offset)?;
            let schema_types = graphql_hir::schema_types(db, project_files);

            let field_name = crate::helpers::find_field_name_at_offset(block_context.tree, offset)?;

            let parent_type_name = symbol::walk_type_stack_to_offset(
                block_context.tree,
                schema_types,
                offset,
                &parent_context.root_type,
            )?;

            let type_def = schema_types.get(parent_type_name.as_str())?;
            let field = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == field_name)?;
            let arg = field.arguments.iter().find(|a| a.name.as_ref() == name)?;

            let file_path = registry.get_path(field.file_id)?;
            let content = registry.get_content(field.file_id)?;
            let line_index = graphql_syntax::line_index(db, content);
            let start: usize = arg.name_range.start().into();
            let end: usize = arg.name_range.end().into();
            let range = offset_range_to_range(&line_index, start, end);

            Some(vec![Location::new(file_path, range)])
        }
        Symbol::OperationName { name } => {
            let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
            let range = find_operation_definition_in_tree(
                block_context.tree,
                &name,
                &block_line_index,
                block_context.line_offset,
            );

            if let Some(range) = range {
                let file_id = registry.get_file_id(file)?;
                let file_path = registry.get_path(file_id)?;
                return Some(vec![Location::new(file_path, range)]);
            }
            None
        }
    }
}
