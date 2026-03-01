//! Find references feature implementation.
//!
//! This module provides IDE find references functionality for:
//! - Fragment references (spreads and definitions)
//! - Type references (in schema and documents)
//! - Field references (definitions and usages)

use std::sync::Arc;

use crate::helpers::{
    find_block_for_position, find_field_usages_in_parse, find_fragment_definition_in_parse,
    find_fragment_spreads_in_parse, find_type_definition_in_parse, find_type_references_in_parse,
    offset_range_to_range, position_to_offset,
};
use crate::symbol::{find_schema_field_parent_type, find_symbol_at_offset, Symbol};
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

        // Pre-filter: skip files that don't reference this fragment.
        // file_used_fragment_names is a cached per-file query, avoiding
        // expensive parse + CST scan for files that don't use the fragment.
        let used_names = graphql_hir::file_used_fragment_names(db, *file_id, content, metadata);
        if !used_names.contains(fragment_name) {
            continue;
        }

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

    // O(1) declaration lookup using HIR source locations instead of
    // linear scanning all schema files with CST walking.
    if include_declaration {
        if let Some(type_def) = schema_types.get(type_name) {
            if let Some(field_sig) = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == field_name)
            {
                let field_file_id = type_def.file_id;
                let file_path = registry.get_path(field_file_id);

                if let (Some(file_path), Some((content, _metadata))) = (
                    file_path,
                    graphql_base_db::file_lookup(db, project_files, field_file_id),
                ) {
                    let source_text: &str = &content.text(db);
                    let line_index = graphql_syntax::LineIndex::new(source_text);
                    let start = u32::from(field_sig.name_range.start()) as usize;
                    let end = u32::from(field_sig.name_range.end()) as usize;
                    let range = offset_range_to_range(&line_index, start, end);
                    locations.push(Location::new(file_path, range));
                }
            }
        }
    }

    // Build target coordinates for pre-filtering document files.
    // For interface fields, expand targets to include implementing types.
    let field_name_arc: Arc<str> = Arc::from(field_name);
    let type_name_arc: Arc<str> = Arc::from(type_name);
    let mut target_coords = vec![graphql_hir::SchemaCoordinate {
        type_name: type_name_arc.clone(),
        field_name: field_name_arc.clone(),
    }];
    if let Some(type_def) = schema_types.get(type_name) {
        if type_def.kind == graphql_hir::TypeDefKind::Interface {
            let implementors = graphql_hir::interface_implementors(db, project_files);
            if let Some(impl_types) = implementors.get(&type_name_arc) {
                for impl_type in impl_types {
                    target_coords.push(graphql_hir::SchemaCoordinate {
                        type_name: impl_type.clone(),
                        field_name: field_name_arc.clone(),
                    });
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

        // Pre-filter: skip files that don't reference this type.field
        // (or implementor.field for interface fields).
        let file_coords =
            graphql_hir::file_schema_coordinates(db, *file_id, content, metadata, project_files);
        if !target_coords.iter().any(|tc| file_coords.contains(tc)) {
            continue;
        }

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
