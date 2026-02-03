//! Completion feature implementation.
//!
//! This module provides IDE auto-completion functionality including:
//! - Field completions in selection sets
//! - Fragment spread completions
//! - Inline fragment completions for unions and interfaces

use crate::helpers::{find_block_for_position, format_type_ref, position_to_offset};
use crate::symbol::{
    find_parent_type_at_offset, find_symbol_at_offset, is_in_selection_set, Symbol,
};
use crate::types::{CompletionItem, CompletionKind, FilePath, InsertTextFormat, Position};
use crate::FileRegistry;

/// Get completions at a position.
///
/// Returns a list of completion items appropriate for the context.
#[allow(clippy::too_many_lines)]
pub fn completions(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
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

    let (block_context, adjusted_position) = find_block_for_position(&parse, position)?;

    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;

    let symbol = find_symbol_at_offset(block_context.tree, offset);

    match symbol {
        Some(Symbol::FragmentSpread { .. }) => {
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
            let Some(project_files) = project_files else {
                return Some(Vec::new());
            };
            let types = graphql_hir::schema_types(db, project_files);

            let in_selection_set = is_in_selection_set(block_context.tree, offset);
            if in_selection_set {
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

                        let mut items: Vec<CompletionItem> = parent_type
                            .fields
                            .iter()
                            .map(|field| {
                                CompletionItem::new(field.name.to_string(), CompletionKind::Field)
                                    .with_detail(format_type_ref(&field.type_ref))
                            })
                            .collect();

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
                Some(Vec::new())
            }
        }
        _ => Some(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_item_new() {
        let item = CompletionItem::new("name".to_string(), CompletionKind::Field);
        assert_eq!(item.label, "name");
        assert_eq!(item.kind, CompletionKind::Field);
        assert!(item.detail.is_none());
        assert!(item.documentation.is_none());
    }

    #[test]
    fn test_completion_item_with_detail() {
        let item = CompletionItem::new("id".to_string(), CompletionKind::Field)
            .with_detail("ID!".to_string());
        assert_eq!(item.detail, Some("ID!".to_string()));
    }

    #[test]
    fn test_completion_item_with_insert_text() {
        let item = CompletionItem::new("UserFields".to_string(), CompletionKind::Fragment)
            .with_insert_text("...UserFields".to_string());
        assert_eq!(item.insert_text, Some("...UserFields".to_string()));
    }

    #[test]
    fn test_completion_item_with_insert_text_format() {
        let item = CompletionItem::new("inline".to_string(), CompletionKind::Type)
            .with_insert_text_format(InsertTextFormat::Snippet);
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::Snippet));
    }

    #[test]
    fn test_completion_kind_variants() {
        assert_eq!(CompletionKind::Field, CompletionKind::Field);
        assert_eq!(CompletionKind::Type, CompletionKind::Type);
        assert_eq!(CompletionKind::Fragment, CompletionKind::Fragment);
        assert_eq!(CompletionKind::Directive, CompletionKind::Directive);
        assert_eq!(CompletionKind::EnumValue, CompletionKind::EnumValue);
    }

    #[test]
    fn test_insert_text_format_variants() {
        assert_eq!(InsertTextFormat::PlainText, InsertTextFormat::PlainText);
        assert_eq!(InsertTextFormat::Snippet, InsertTextFormat::Snippet);
    }

    #[test]
    fn test_completion_item_chaining() {
        let item = CompletionItem::new("user".to_string(), CompletionKind::Field)
            .with_detail("User!".to_string())
            .with_sort_text("aaa_user".to_string());

        assert_eq!(item.label, "user");
        assert_eq!(item.detail, Some("User!".to_string()));
        assert_eq!(item.sort_text, Some("aaa_user".to_string()));
    }
}
