//! Document and workspace symbols feature implementation.
//!
//! This module provides IDE symbol functionality:
//! - Document symbols (Cmd+Shift+O) - hierarchical outline of a file
//! - Workspace symbols (Cmd+T) - search across all files

use crate::helpers::{adjust_range_for_line_offset, format_type_ref, offset_range_to_range};
use crate::symbol::{
    extract_all_definitions, find_field_definition_full_range, find_fragment_definition_full_range,
    find_operation_definition_ranges, find_type_definition_full_range,
};
use crate::types::{DocumentSymbol, FilePath, Location, SymbolKind, WorkspaceSymbol};
use crate::FileRegistry;

/// Get document symbols for a file (hierarchical outline).
///
/// Returns types, operations, and fragments with their fields as children.
/// This powers the "Go to Symbol in Editor" (Cmd+Shift+O) feature.
#[allow(clippy::too_many_lines)]
pub fn document_symbols(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    file: &FilePath,
) -> Vec<DocumentSymbol> {
    let (content, metadata, file_id) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata, file_id)
    };

    let parse = graphql_syntax::parse(db, content, metadata);

    let structure = graphql_hir::file_structure(db, file_id, content, metadata);

    let mut symbols = Vec::new();

    for doc in parse.documents() {
        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
        #[allow(clippy::cast_possible_truncation)]
        let doc_line_offset = doc.line_offset as u32;

        let definitions = extract_all_definitions(doc.tree);

        for (name, kind, ranges) in definitions {
            let range = adjust_range_for_line_offset(
                offset_range_to_range(&doc_line_index, ranges.def_start, ranges.def_end),
                doc_line_offset,
            );
            let selection_range = adjust_range_for_line_offset(
                offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end),
                doc_line_offset,
            );

            let symbol = match kind {
                "object" => {
                    let children = get_field_children(
                        &structure,
                        &name,
                        doc.tree,
                        &doc_line_index,
                        doc_line_offset,
                    );
                    DocumentSymbol::new(name, SymbolKind::Type, range, selection_range)
                        .with_children(children)
                }
                "interface" => {
                    let children = get_field_children(
                        &structure,
                        &name,
                        doc.tree,
                        &doc_line_index,
                        doc_line_offset,
                    );
                    DocumentSymbol::new(name, SymbolKind::Interface, range, selection_range)
                        .with_children(children)
                }
                "input" => {
                    let children = get_field_children(
                        &structure,
                        &name,
                        doc.tree,
                        &doc_line_index,
                        doc_line_offset,
                    );
                    DocumentSymbol::new(name, SymbolKind::Input, range, selection_range)
                        .with_children(children)
                }
                "union" => DocumentSymbol::new(name, SymbolKind::Union, range, selection_range),
                "enum" => DocumentSymbol::new(name, SymbolKind::Enum, range, selection_range),
                "scalar" => DocumentSymbol::new(name, SymbolKind::Scalar, range, selection_range),
                "query" => DocumentSymbol::new(name, SymbolKind::Query, range, selection_range),
                "mutation" => {
                    DocumentSymbol::new(name, SymbolKind::Mutation, range, selection_range)
                }
                "subscription" => {
                    DocumentSymbol::new(name, SymbolKind::Subscription, range, selection_range)
                }
                "fragment" => {
                    let detail = structure
                        .fragments
                        .iter()
                        .find(|f| f.name.as_ref() == name)
                        .map(|f| format!("on {}", f.type_condition));
                    let mut sym =
                        DocumentSymbol::new(name, SymbolKind::Fragment, range, selection_range);
                    if let Some(d) = detail {
                        sym = sym.with_detail(d);
                    }
                    sym
                }
                _ => continue,
            };

            symbols.push(symbol);
        }
    }

    symbols
}

/// Search for workspace symbols matching a query.
///
/// Returns matching types, operations, and fragments across all files.
/// This powers the "Go to Symbol in Workspace" (Cmd+T) feature.
pub fn workspace_symbols(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    query: &str,
) -> Vec<WorkspaceSymbol> {
    let Some(project_files) = project_files else {
        return Vec::new();
    };

    let query_lower = query.to_lowercase();
    let mut symbols = Vec::new();

    let types = graphql_hir::schema_types(db, project_files);
    for (name, type_def) in types {
        if name.to_lowercase().contains(&query_lower) {
            if let Some(location) = get_type_location(db, registry, type_def) {
                #[allow(clippy::match_same_arms)]
                let kind = match type_def.kind {
                    graphql_hir::TypeDefKind::Object => SymbolKind::Type,
                    graphql_hir::TypeDefKind::Interface => SymbolKind::Interface,
                    graphql_hir::TypeDefKind::Union => SymbolKind::Union,
                    graphql_hir::TypeDefKind::Enum => SymbolKind::Enum,
                    graphql_hir::TypeDefKind::Scalar => SymbolKind::Scalar,
                    graphql_hir::TypeDefKind::InputObject => SymbolKind::Input,
                    _ => SymbolKind::Type,
                };

                symbols.push(WorkspaceSymbol::new(name.to_string(), kind, location));
            }
        }
    }

    let fragments = graphql_hir::all_fragments(db, project_files);
    for (name, fragment) in fragments {
        if name.to_lowercase().contains(&query_lower) {
            if let Some(location) = get_fragment_location(db, registry, fragment) {
                symbols.push(
                    WorkspaceSymbol::new(name.to_string(), SymbolKind::Fragment, location)
                        .with_container(format!("on {}", fragment.type_condition)),
                );
            }
        }
    }

    let doc_ids = project_files.document_file_ids(db).ids(db);
    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };
        let structure = graphql_hir::file_structure(db, *file_id, content, metadata);
        for operation in structure.operations.iter() {
            if let Some(op_name) = &operation.name {
                if op_name.to_lowercase().contains(&query_lower) {
                    if let Some(location) = get_operation_location(db, registry, operation) {
                        #[allow(clippy::match_same_arms)]
                        let kind = match operation.operation_type {
                            graphql_hir::OperationType::Query => SymbolKind::Query,
                            graphql_hir::OperationType::Mutation => SymbolKind::Mutation,
                            graphql_hir::OperationType::Subscription => SymbolKind::Subscription,
                            _ => SymbolKind::Query,
                        };

                        symbols.push(WorkspaceSymbol::new(op_name.to_string(), kind, location));
                    }
                }
            }
        }
    }

    symbols
}

/// Get field children for a type definition.
fn get_field_children(
    structure: &graphql_hir::FileStructureData,
    type_name: &str,
    tree: &apollo_parser::SyntaxTree,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Vec<DocumentSymbol> {
    let Some(type_def) = structure
        .type_defs
        .iter()
        .find(|t| t.name.as_ref() == type_name)
    else {
        return Vec::new();
    };

    let mut children = Vec::new();

    for field in &type_def.fields {
        if let Some(ranges) = find_field_definition_full_range(tree, type_name, &field.name) {
            let range = adjust_range_for_line_offset(
                offset_range_to_range(line_index, ranges.def_start, ranges.def_end),
                line_offset,
            );
            let selection_range = adjust_range_for_line_offset(
                offset_range_to_range(line_index, ranges.name_start, ranges.name_end),
                line_offset,
            );

            let detail = format_type_ref(&field.type_ref);
            children.push(
                DocumentSymbol::new(
                    field.name.to_string(),
                    SymbolKind::Field,
                    range,
                    selection_range,
                )
                .with_detail(detail),
            );
        }
    }

    children
}

/// Get location for a type definition.
fn get_type_location(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    type_def: &graphql_hir::TypeDef,
) -> Option<Location> {
    let file_path = registry.get_path(type_def.file_id)?;
    let content = registry.get_content(type_def.file_id)?;
    let metadata = registry.get_metadata(type_def.file_id)?;

    let parse = graphql_syntax::parse(db, content, metadata);

    for doc in parse.documents() {
        if let Some(ranges) = find_type_definition_full_range(doc.tree, &type_def.name) {
            let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
            #[allow(clippy::cast_possible_truncation)]
            let range = adjust_range_for_line_offset(
                offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end),
                doc.line_offset as u32,
            );
            return Some(Location::new(file_path, range));
        }
    }

    None
}

/// Get location for a fragment definition.
fn get_fragment_location(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    fragment: &graphql_hir::FragmentStructure,
) -> Option<Location> {
    let file_path = registry.get_path(fragment.file_id)?;
    let content = registry.get_content(fragment.file_id)?;
    let metadata = registry.get_metadata(fragment.file_id)?;

    let parse = graphql_syntax::parse(db, content, metadata);

    for doc in parse.documents() {
        if let Some(ranges) = find_fragment_definition_full_range(doc.tree, &fragment.name) {
            let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
            #[allow(clippy::cast_possible_truncation)]
            let range = adjust_range_for_line_offset(
                offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end),
                doc.line_offset as u32,
            );
            return Some(Location::new(file_path, range));
        }
    }

    None
}

/// Get location for an operation definition.
fn get_operation_location(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    operation: &graphql_hir::OperationStructure,
) -> Option<Location> {
    let op_name = operation.name.as_ref()?;

    let file_path = registry.get_path(operation.file_id)?;
    let content = registry.get_content(operation.file_id)?;
    let metadata = registry.get_metadata(operation.file_id)?;

    let parse = graphql_syntax::parse(db, content, metadata);

    for doc in parse.documents() {
        if let Some(ranges) = find_operation_definition_ranges(doc.tree, op_name) {
            let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
            #[allow(clippy::cast_possible_truncation)]
            let range = adjust_range_for_line_offset(
                offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end),
                doc.line_offset as u32,
            );
            return Some(Location::new(file_path, range));
        }
    }

    None
}
