//! Document and workspace symbols feature implementation.
//!
//! This module provides IDE symbol functionality:
//! - Document symbols (Cmd+Shift+O) - hierarchical outline of a file
//! - Workspace symbols (Cmd+T) - search across all files

use std::collections::HashMap;

use crate::helpers::{adjust_range_for_line_offset, format_type_ref, offset_range_to_range};
use crate::symbol::{
    extract_all_definitions, find_fragment_definition_full_range, find_operation_definition_ranges,
    find_type_definition_full_range, SymbolRanges,
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
        let field_ranges_map = extract_all_field_ranges(doc.tree);

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
                "object" | "interface" | "input" => {
                    let children = get_field_children_from_map(
                        &structure,
                        &name,
                        &field_ranges_map,
                        &doc_line_index,
                        doc_line_offset,
                    );
                    let sym_kind = match kind {
                        "object" => SymbolKind::Type,
                        "interface" => SymbolKind::Interface,
                        _ => SymbolKind::Input,
                    };
                    DocumentSymbol::new(name, sym_kind, range, selection_range)
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

/// Extract field ranges for all type definitions in a single AST pass.
///
/// Returns a map of type name to field name/range pairs. This avoids the
/// per-field AST walk that causes O(n^3) behavior and hangs the LSP on
/// large generated schema files.
fn extract_all_field_ranges(
    tree: &apollo_parser::SyntaxTree,
) -> HashMap<String, Vec<(String, SymbolRanges)>> {
    use apollo_parser::cst::{self, CstNode};

    let doc = tree.document();
    let mut map: HashMap<String, Vec<(String, SymbolRanges)>> = HashMap::new();

    for definition in doc.definitions() {
        match &definition {
            cst::Definition::ObjectTypeDefinition(obj) => {
                if let (Some(name), Some(fields_def)) = (obj.name(), obj.fields_definition()) {
                    map.insert(
                        name.text().to_string(),
                        collect_field_ranges(fields_def.field_definitions()),
                    );
                }
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                if let (Some(name), Some(fields_def)) = (iface.name(), iface.fields_definition()) {
                    map.insert(
                        name.text().to_string(),
                        collect_field_ranges(fields_def.field_definitions()),
                    );
                }
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                if let (Some(name), Some(fields_def)) =
                    (input.name(), input.input_fields_definition())
                {
                    let field_ranges: Vec<(String, SymbolRanges)> = fields_def
                        .input_value_definitions()
                        .filter_map(|field| {
                            let field_name = field.name()?;
                            let name_range = field_name.syntax().text_range();
                            let def_range = field.syntax().text_range();
                            Some((
                                field_name.text().to_string(),
                                SymbolRanges {
                                    name_start: name_range.start().into(),
                                    name_end: name_range.end().into(),
                                    def_start: def_range.start().into(),
                                    def_end: def_range.end().into(),
                                },
                            ))
                        })
                        .collect();
                    map.insert(name.text().to_string(), field_ranges);
                }
            }
            _ => {}
        }
    }

    map
}

/// Collect field name/range pairs from a `FieldDefinition` iterator.
fn collect_field_ranges(
    fields: impl Iterator<Item = apollo_parser::cst::FieldDefinition>,
) -> Vec<(String, SymbolRanges)> {
    use apollo_parser::cst::CstNode;

    fields
        .filter_map(|field| {
            let name = field.name()?;
            let name_range = name.syntax().text_range();
            let def_range = field.syntax().text_range();
            Some((
                name.text().to_string(),
                SymbolRanges {
                    name_start: name_range.start().into(),
                    name_end: name_range.end().into(),
                    def_start: def_range.start().into(),
                    def_end: def_range.end().into(),
                },
            ))
        })
        .collect()
}

/// Get field children for a type definition using pre-extracted field ranges.
fn get_field_children_from_map(
    structure: &graphql_hir::FileStructureData,
    type_name: &str,
    field_ranges_map: &HashMap<String, Vec<(String, SymbolRanges)>>,
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

    let Some(field_ranges) = field_ranges_map.get(type_name) else {
        return Vec::new();
    };

    let mut children = Vec::new();

    for field in &type_def.fields {
        if let Some((_, ranges)) = field_ranges.iter().find(|(n, _)| n == field.name.as_ref()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Position, Range};

    fn test_range() -> Range {
        Range::new(Position::new(0, 0), Position::new(0, 10))
    }

    #[test]
    fn test_document_symbol_new() {
        let range = Range::new(Position::new(0, 0), Position::new(5, 0));
        let selection_range = Range::new(Position::new(0, 5), Position::new(0, 9));

        let symbol =
            DocumentSymbol::new("User".to_string(), SymbolKind::Type, range, selection_range);

        assert_eq!(symbol.name, "User");
        assert_eq!(symbol.kind, SymbolKind::Type);
        assert!(symbol.children.is_empty());
        assert!(symbol.detail.is_none());
    }

    #[test]
    fn test_document_symbol_with_children() {
        let range = test_range();

        let child = DocumentSymbol::new("id".to_string(), SymbolKind::Field, range, range);
        let parent = DocumentSymbol::new("User".to_string(), SymbolKind::Type, range, range)
            .with_children(vec![child]);

        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name, "id");
    }

    #[test]
    fn test_document_symbol_with_detail() {
        let range = test_range();
        let symbol =
            DocumentSymbol::new("UserFields".to_string(), SymbolKind::Fragment, range, range)
                .with_detail("on User".to_string());

        assert_eq!(symbol.detail, Some("on User".to_string()));
    }

    #[test]
    fn test_workspace_symbol_new() {
        let location = Location::new(FilePath::new("file:///schema.graphql"), test_range());

        let symbol = WorkspaceSymbol::new("User".to_string(), SymbolKind::Type, location);

        assert_eq!(symbol.name, "User");
        assert_eq!(symbol.kind, SymbolKind::Type);
        assert!(symbol.container_name.is_none());
    }

    #[test]
    fn test_workspace_symbol_with_container() {
        let location = Location::new(FilePath::new("file:///fragments.graphql"), test_range());

        let symbol = WorkspaceSymbol::new("UserFields".to_string(), SymbolKind::Fragment, location)
            .with_container("on User".to_string());

        assert_eq!(symbol.container_name, Some("on User".to_string()));
    }

    #[test]
    fn test_symbol_kind_variants() {
        assert_eq!(SymbolKind::Type, SymbolKind::Type);
        assert_eq!(SymbolKind::Field, SymbolKind::Field);
        assert_eq!(SymbolKind::Query, SymbolKind::Query);
        assert_eq!(SymbolKind::Mutation, SymbolKind::Mutation);
        assert_eq!(SymbolKind::Subscription, SymbolKind::Subscription);
        assert_eq!(SymbolKind::Fragment, SymbolKind::Fragment);
        assert_eq!(SymbolKind::Interface, SymbolKind::Interface);
        assert_eq!(SymbolKind::Union, SymbolKind::Union);
        assert_eq!(SymbolKind::Enum, SymbolKind::Enum);
        assert_eq!(SymbolKind::Scalar, SymbolKind::Scalar);
        assert_eq!(SymbolKind::Input, SymbolKind::Input);
    }
}
