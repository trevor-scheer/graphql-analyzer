//! Completion feature implementation.
//!
//! This module provides IDE autocompletion functionality including:
//! - Field completions within selection sets
//! - Fragment name completions
//! - Inline fragment suggestions for unions and interfaces

use crate::helpers::{find_block_for_position, format_type_ref, position_to_offset};
use crate::symbol::{
    find_parent_type_at_offset, find_symbol_at_offset, is_in_selection_set, Symbol,
};
use crate::types::{CompletionItem, CompletionKind, FilePath, InsertTextFormat, Position};
use crate::FileRegistry;

/// Get completions at a position in a file.
///
/// Returns a list of completion items appropriate for the context:
/// - Fields when in a selection set
/// - Fragment names when on a fragment spread
/// - Inline fragments for union/interface types
#[allow(clippy::too_many_lines)]
pub fn completions(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
) -> Option<Vec<CompletionItem>> {
    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;

        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;

        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);

    let metadata_line_offset = metadata.line_offset(db);
    let (block_context, adjusted_position) =
        find_block_for_position(&parse, position, metadata_line_offset)?;

    let offset = if let Some(block_source) = block_context.block_source {
        let block_line_index = graphql_syntax::LineIndex::new(block_source);
        position_to_offset(&block_line_index, adjusted_position)?
    } else {
        let line_index = graphql_syntax::line_index(db, content);
        position_to_offset(&line_index, adjusted_position)?
    };

    // Find what symbol we're completing (or near) using the correct tree
    let symbol = find_symbol_at_offset(block_context.tree, offset);

    // Determine completion context and provide appropriate completions
    match symbol {
        Some(Symbol::FragmentSpread { .. }) => {
            // Complete fragment names when on a fragment spread
            let Some(project_files) = project_files else {
                return Some(Vec::new());
            };
            let fragments = graphql_hir::all_fragments(db, project_files);

            let items: Vec<CompletionItem> = fragments
                .keys()
                .map(|name| CompletionItem::new(name.to_string(), CompletionKind::Fragment))
                .collect();

            Some(items)
        }
        None | Some(Symbol::FieldName { .. }) => {
            // Show fields from parent type in selection set or on field name
            let Some(project_files) = project_files else {
                return Some(Vec::new());
            };
            let types = graphql_hir::schema_types(db, project_files);

            let in_selection_set = is_in_selection_set(block_context.tree, offset);
            if in_selection_set {
                // Use a stack-based type walker to resolve the parent type at the cursor
                let parent_ctx = find_parent_type_at_offset(block_context.tree, offset)?;
                let parent_type_name = crate::symbol::walk_type_stack_to_offset(
                    block_context.tree,
                    types,
                    offset,
                    &parent_ctx.root_type,
                )?;

                types.get(parent_type_name.as_str()).map_or_else(
                    || Some(Vec::new()),
                    |parent_type| {
                        // For union types, suggest inline fragments for each union member
                        if parent_type.kind == graphql_hir::TypeDefKind::Union {
                            let items: Vec<CompletionItem> = parent_type
                                .union_members
                                .iter()
                                .map(|member| {
                                    CompletionItem::new(
                                        format!("... on {member}"),
                                        CompletionKind::Type,
                                    )
                                    .with_insert_text(format!("... on {member} {{\n  $0\n}}"))
                                    .with_insert_text_format(InsertTextFormat::Snippet)
                                })
                                .collect();
                            return Some(items);
                        }

                        // For object types and interfaces, suggest fields
                        let mut items: Vec<CompletionItem> = parent_type
                            .fields
                            .iter()
                            .map(|field| {
                                CompletionItem::new(field.name.to_string(), CompletionKind::Field)
                                    .with_detail(format_type_ref(&field.type_ref))
                            })
                            .collect();

                        // If interface, add inline fragment suggestions for implementing types
                        if parent_type.kind == graphql_hir::TypeDefKind::Interface {
                            for type_def in types.values() {
                                if type_def.implements.contains(&parent_type.name) {
                                    let type_name = &type_def.name;
                                    let inline_fragment_label = format!("... on {type_name}");
                                    if !items
                                        .iter()
                                        .any(|i| i.label.as_str() == inline_fragment_label)
                                    {
                                        items.push(
                                            CompletionItem::new(
                                                inline_fragment_label,
                                                CompletionKind::Type,
                                            )
                                            .with_insert_text(format!(
                                                "... on {type_name} {{\n  $0\n}}"
                                            ))
                                            .with_insert_text_format(InsertTextFormat::Snippet)
                                            .with_sort_text(format!("z_{type_name}")),
                                        );
                                    }
                                }
                            }
                        }
                        Some(items)
                    },
                )
            } else {
                // Not in a selection set - we're at document level
                Some(Vec::new())
            }
        }
        _ => Some(Vec::new()),
    }
}
