//! Goto definition feature implementation.
//!
//! This module provides IDE goto definition functionality for:
//! - Field definitions in schema
//! - Fragment definitions
//! - Type definitions
//! - Variable definitions
//! - Argument definitions
//! - Operation definitions
//!
//! Performance: Uses pre-computed HIR indexes (`schema_types`, `type_definition_locations`)
//! so that each lookup parses at most one file instead of iterating all schema files.

use crate::helpers::{
    adjust_range_for_line_offset, find_argument_definition_in_tree,
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
            tracing::debug!("Goto definition: FieldName '{}', querying schema_types", name);
            let schema_types = graphql_hir::schema_types(db, project_files);
            tracing::debug!("Goto definition: schema_types query complete");

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

            // Look up the field in the merged schema types - O(1) HashMap lookup
            let type_def = schema_types.get(parent_type_name.as_str())?;
            let field = type_def.fields.iter().find(|f| f.name.as_ref() == name)?;
            let field_file_id = field.file_id;

            tracing::debug!(
                "Field '{}.{}' defined in FileId {:?}, looking up in registry",
                parent_type_name,
                name,
                field_file_id
            );

            // Use the field's file_id to parse only the single file containing this field
            let Some(file_path) = registry.get_path(field_file_id) else {
                tracing::error!(
                    "FileId {:?} not found in registry for field '{}.{}'",
                    field_file_id,
                    parent_type_name,
                    name
                );
                return None;
            };
            let field_content = registry.get_content(field_file_id)?;
            let field_metadata = registry.get_metadata(field_file_id)?;

            tracing::debug!("Parsing file for field definition lookup");
            let field_parse = graphql_syntax::parse(db, field_content, field_metadata);

            for doc in field_parse.documents() {
                if let Some(ranges) = crate::symbol::find_field_definition_full_range(
                    doc.tree,
                    &parent_type_name,
                    &name,
                ) {
                    let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                    let range =
                        offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end);
                    let adjusted_range = adjust_range_for_line_offset(range, doc.line_offset);
                    tracing::debug!("Found field definition at {:?}", adjusted_range);
                    return Some(vec![Location::new(file_path, adjusted_range)]);
                }
            }

            tracing::debug!(
                "Field '{}.{}' not found via CST search in FileId {:?}",
                parent_type_name,
                name,
                field_file_id
            );
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
            // Use the pre-computed type definition location index - O(1) lookup
            let locations_index = graphql_hir::type_definition_locations(db, project_files);
            let type_locations = locations_index.get(name.as_str())?;

            let mut locations = Vec::new();

            for type_loc in type_locations {
                let Some(file_path) = registry.get_path(type_loc.file_id) else {
                    continue;
                };
                let Some(type_content) = registry.get_content(type_loc.file_id) else {
                    continue;
                };
                let Some(type_metadata) = registry.get_metadata(type_loc.file_id) else {
                    continue;
                };

                // Parse only the file that contains this type definition
                let type_parse = graphql_syntax::parse(db, type_content, type_metadata);

                for range in crate::helpers::find_all_type_definitions_in_parse(&type_parse, &name)
                {
                    locations.push(Location::new(file_path.clone(), range));
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

            // Use schema_types to find the field and its file_id - O(1) lookup
            let type_def = schema_types.get(parent_type_name.as_str())?;
            let field = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == field_name)?;

            // Parse only the single file containing this field definition
            let file_path = registry.get_path(field.file_id)?;
            let field_content = registry.get_content(field.file_id)?;
            let field_metadata = registry.get_metadata(field.file_id)?;

            let field_parse = graphql_syntax::parse(db, field_content, field_metadata);

            for doc in field_parse.documents() {
                let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                if let Some(range) = find_argument_definition_in_tree(
                    doc.tree,
                    &parent_type_name,
                    &field_name,
                    &name,
                    &doc_line_index,
                    doc.line_offset,
                ) {
                    return Some(vec![Location::new(file_path, range)]);
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
