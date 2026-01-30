//! Find references feature implementation.
//!
//! This module provides IDE find references functionality for:
//! - Fragment references (spreads and definitions)
//! - Type references (in schema and documents)
//! - Field references (definitions and usages)

use crate::helpers::{
    adjust_range_for_line_offset, find_block_for_position, find_field_usages_in_parse,
    find_fragment_definition_in_parse, find_fragment_spreads_in_parse,
    find_type_definition_in_parse, find_type_references_in_parse, offset_range_to_range,
    position_to_offset,
};
use crate::symbol::{
    find_field_definition_full_range, find_schema_field_parent_type, find_symbol_at_offset, Symbol,
};
use crate::types::{FilePath, Location, Position};
use crate::FileRegistry;

/// Find all references to the symbol at a position.
///
/// Returns locations of all usages of types, fields, fragments, etc.
pub fn find_references(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
    include_declaration: bool,
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
        "Find references: original position {:?}, block line_offset {}, adjusted position {:?}",
        position,
        block_context.line_offset,
        adjusted_position
    );

    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;

    let symbol = find_symbol_at_offset(block_context.tree, offset)?;

    match symbol {
        Symbol::FragmentSpread { name } => Some(find_fragment_references(
            db,
            registry,
            project_files,
            &name,
            include_declaration,
        )),
        Symbol::TypeName { name } => Some(find_type_references(
            db,
            registry,
            project_files,
            &name,
            include_declaration,
        )),
        Symbol::FieldName { name } => {
            let parent_type = find_schema_field_parent_type(block_context.tree, offset)?;
            Some(find_field_references(
                db,
                registry,
                project_files,
                &parent_type,
                &name,
                include_declaration,
            ))
        }
        _ => None,
    }
}

/// Find all references to a fragment.
pub fn find_fragment_references(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    fragment_name: &str,
    include_declaration: bool,
) -> Vec<Location> {
    let mut locations = Vec::new();

    let Some(project_files) = project_files else {
        return locations;
    };

    let fragments = graphql_hir::all_fragments(db, project_files);

    if include_declaration {
        if let Some(fragment) = fragments.get(fragment_name) {
            let file_path = registry.get_path(fragment.file_id);
            let def_content = registry.get_content(fragment.file_id);
            let def_metadata = registry.get_metadata(fragment.file_id);

            if let (Some(file_path), Some(def_content), Some(def_metadata)) =
                (file_path, def_content, def_metadata)
            {
                let def_parse = graphql_syntax::parse(db, def_content, def_metadata);

                if let Some(range) =
                    find_fragment_definition_in_parse(&def_parse, fragment_name, def_content, db)
                {
                    locations.push(Location::new(file_path, range));
                }
            }
        }
    }

    let doc_ids = project_files.document_file_ids(db).ids(db);

    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };

        let file_path = registry.get_path(*file_id);

        let Some(file_path) = file_path else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);

        let spread_ranges = find_fragment_spreads_in_parse(&parse, fragment_name, content, db);

        for range in spread_ranges {
            locations.push(Location::new(file_path.clone(), range));
        }
    }

    locations
}

/// Find all references to a type.
fn find_type_references(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    type_name: &str,
    include_declaration: bool,
) -> Vec<Location> {
    let mut locations = Vec::new();

    let Some(project_files) = project_files else {
        return locations;
    };

    let types = graphql_hir::schema_types(db, project_files);

    if include_declaration {
        if let Some(type_def) = types.get(type_name) {
            let file_path = registry.get_path(type_def.file_id);
            let def_content = registry.get_content(type_def.file_id);
            let def_metadata = registry.get_metadata(type_def.file_id);

            if let (Some(file_path), Some(def_content), Some(def_metadata)) =
                (file_path, def_content, def_metadata)
            {
                let def_parse = graphql_syntax::parse(db, def_content, def_metadata);

                if let Some(range) =
                    find_type_definition_in_parse(&def_parse, type_name, def_content, db)
                {
                    locations.push(Location::new(file_path, range));
                }
            }
        }
    }

    let schema_ids = project_files.schema_file_ids(db).ids(db);

    for file_id in schema_ids.iter() {
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };

        let file_path = registry.get_path(*file_id);

        let Some(file_path) = file_path else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);

        let type_ranges = find_type_references_in_parse(&parse, type_name, content, db);

        for range in type_ranges {
            locations.push(Location::new(file_path.clone(), range));
        }
    }

    locations
}

/// Find all references to a field on a specific type.
pub fn find_field_references(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    type_name: &str,
    field_name: &str,
    include_declaration: bool,
) -> Vec<Location> {
    let mut locations = Vec::new();

    let Some(project_files) = project_files else {
        return locations;
    };

    let schema_types = graphql_hir::schema_types(db, project_files);

    if include_declaration {
        let schema_ids = project_files.schema_file_ids(db).ids(db);

        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            let file_path = registry.get_path(*file_id);

            let Some(file_path) = file_path else {
                continue;
            };

            let parse = graphql_syntax::parse(db, content, metadata);

            'schema_search: for doc in parse.documents() {
                if let Some(ranges) =
                    find_field_definition_full_range(doc.tree, type_name, field_name)
                {
                    let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                    let range =
                        offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end);
                    #[allow(clippy::cast_possible_truncation)]
                    let adjusted_range =
                        adjust_range_for_line_offset(range, doc.line_offset as u32);
                    locations.push(Location::new(file_path, adjusted_range));
                    break 'schema_search;
                }
            }
        }
    }

    let doc_ids = project_files.document_file_ids(db).ids(db);

    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };

        let file_path = registry.get_path(*file_id);

        let Some(file_path) = file_path else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);

        let field_ranges =
            find_field_usages_in_parse(&parse, type_name, field_name, schema_types, content, db);

        for range in field_ranges {
            locations.push(Location::new(file_path.clone(), range));
        }
    }

    locations
}
