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
    adjust_range_for_line_offset, find_argument_definition_in_tree,
    find_fragment_definition_in_parse, find_operation_definition_in_tree,
    find_type_definition_in_parse, find_variable_definition_in_tree, offset_range_to_range,
    position_to_offset,
};
use crate::symbol::{
    find_field_definition_full_range, find_parent_type_at_offset, find_symbol_at_offset, Symbol,
};
use crate::types::{FilePath, Location, Position};
use crate::{helpers::find_block_for_position, symbol, FileRegistry};

/// Get goto definition locations for the symbol at a position.
///
/// Returns the definition location(s) for types, fields, fragments, etc.
#[allow(clippy::too_many_lines)]
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

                for doc in schema_parse.documents() {
                    if let Some(ranges) =
                        find_field_definition_full_range(doc.tree, &parent_type_name, &name)
                    {
                        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                        let range = offset_range_to_range(
                            &doc_line_index,
                            ranges.name_start,
                            ranges.name_end,
                        );
                        #[allow(clippy::cast_possible_truncation)]
                        let adjusted_range =
                            adjust_range_for_line_offset(range, doc.line_offset as u32);
                        return Some(vec![Location::new(file_path, adjusted_range)]);
                    }
                }
            }

            None
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

                if let Some(range) =
                    find_type_definition_in_parse(&schema_parse, &name, schema_content, db)
                {
                    return Some(vec![Location::new(file_path, range)]);
                }
            }

            None
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

                for doc in schema_parse.documents() {
                    let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                    #[allow(clippy::cast_possible_truncation)]
                    if let Some(range) = find_argument_definition_in_tree(
                        doc.tree,
                        &parent_type_name,
                        &field_name,
                        &name,
                        &doc_line_index,
                        doc.line_offset as u32,
                    ) {
                        return Some(vec![Location::new(file_path, range)]);
                    }
                }
            }
            None
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
