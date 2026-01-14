//! Find References feature implementation.
//!
//! This module provides IDE "find all references" functionality for:
//! - Fragment spreads (all usages of a fragment)
//! - Type references (all usages of a type in schema)
//! - Field references (all usages of a field in queries)

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
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
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
    let line_index = graphql_syntax::line_index(db, content);

    let metadata_line_offset = metadata.line_offset(db);
    let (block_context, adjusted_position) =
        find_block_for_position(&parse, position, metadata_line_offset)?;

    tracing::debug!(
        "Find references: original position {:?}, block line_offset {}, adjusted position {:?}",
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
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
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
                let def_line_offset = def_metadata.line_offset(db);

                if let Some(range) = find_fragment_definition_in_parse(
                    &def_parse,
                    fragment_name,
                    def_content,
                    db,
                    def_line_offset,
                ) {
                    locations.push(Location::new(file_path, range));
                }
            }
        }
    }

    // Search through all document files for fragment spreads
    let doc_ids = project_files.document_file_ids(db).ids(db);

    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) else {
            continue;
        };

        let file_path = registry.get_path(*file_id);

        let Some(file_path) = file_path else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);
        let line_offset = metadata.line_offset(db);

        let spread_ranges =
            find_fragment_spreads_in_parse(&parse, fragment_name, content, db, line_offset);

        for range in spread_ranges {
            locations.push(Location::new(file_path.clone(), range));
        }
    }

    locations
}

/// Find all references to a type.
pub fn find_type_references(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
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
                let def_line_offset = def_metadata.line_offset(db);

                if let Some(range) = find_type_definition_in_parse(
                    &def_parse,
                    type_name,
                    def_content,
                    db,
                    def_line_offset,
                ) {
                    locations.push(Location::new(file_path, range));
                }
            }
        }
    }

    let schema_ids = project_files.schema_file_ids(db).ids(db);

    for file_id in schema_ids.iter() {
        let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) else {
            continue;
        };

        let file_path = registry.get_path(*file_id);

        let Some(file_path) = file_path else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);
        let line_offset = metadata.line_offset(db);

        let type_ranges =
            find_type_references_in_parse(&parse, type_name, content, db, line_offset);

        for range in type_ranges {
            locations.push(Location::new(file_path.clone(), range));
        }
    }

    locations
}

/// Find all references to a field on a specific type.
pub fn find_field_references(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
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
            let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            let file_path = registry.get_path(*file_id);

            let Some(file_path) = file_path else {
                continue;
            };

            let parse = graphql_syntax::parse(db, content, metadata);
            let line_index = graphql_syntax::line_index(db, content);
            let line_offset = metadata.line_offset(db);

            if let Some(ranges) =
                find_field_definition_full_range(&parse.tree, type_name, field_name)
            {
                let range = offset_range_to_range(&line_index, ranges.name_start, ranges.name_end);
                let adjusted_range = adjust_range_for_line_offset(range, line_offset);
                locations.push(Location::new(file_path, adjusted_range));
                break;
            }
        }
    }

    // Search through all document files for field usages
    let doc_ids = project_files.document_file_ids(db).ids(db);

    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) else {
            continue;
        };

        let file_path = registry.get_path(*file_id);

        let Some(file_path) = file_path else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);
        let line_offset = metadata.line_offset(db);

        let field_ranges = find_field_usages_in_parse(
            &parse,
            type_name,
            field_name,
            schema_types,
            content,
            db,
            line_offset,
        );

        for range in field_ranges {
            locations.push(Location::new(file_path.clone(), range));
        }
    }

    locations
}
