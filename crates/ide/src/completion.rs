//! Completion feature implementation.
//!
//! This module provides IDE auto-completion functionality including:
//! - Field completions in selection sets
//! - Fragment spread completions
//! - Inline fragment completions for unions and interfaces
//! - Argument completions for fields
//! - Enum value completions in argument positions
//! - Directive completions after `@`

use crate::helpers::{
    find_argument_context_at_offset, find_block_for_position, find_operation_variables_at_offset,
    format_type_ref, position_to_offset,
};
use crate::symbol::{
    find_parent_type_at_offset, find_symbol_at_offset, is_in_selection_set, Symbol,
};
use crate::types::{CompletionItem, CompletionKind, FilePath, InsertTextFormat, Position};
use crate::FileRegistry;

/// Built-in GraphQL directives available in all schemas.
const BUILTIN_DIRECTIVES: &[(&str, &str, &str)] = &[
    ("skip", "Directs the executor to skip this field or fragment when the `if` argument is true.", "FIELD | INLINE_FRAGMENT | FRAGMENT_SPREAD"),
    ("include", "Directs the executor to include this field or fragment only when the `if` argument is true.", "FIELD | INLINE_FRAGMENT | FRAGMENT_SPREAD"),
    ("deprecated", "Marks an element of a GraphQL schema as no longer supported.", "FIELD_DEFINITION | ARGUMENT_DEFINITION | INPUT_FIELD_DEFINITION | ENUM_VALUE"),
    ("specifiedBy", "Exposes a URL that specifies the behavior of this scalar.", "SCALAR"),
];

/// Get completions at a position.
///
/// Returns a list of completion items appropriate for the context.
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

    // Check if cursor follows `@` - offer directive completions
    if is_after_at_sign(block_context.block_source, offset) {
        return Some(directive_completions());
    }

    // Check if cursor follows `$` - offer variable completions
    if is_after_dollar_sign(block_context.block_source, offset) {
        return Some(variable_completions(block_context.tree, offset));
    }

    // Check if cursor is in a type name position (after `on` keyword or after `:` in variable def)
    if is_in_type_position(block_context.block_source, offset) {
        if let Some(project_files) = project_files {
            let types = graphql_hir::schema_types(db, project_files);
            return Some(type_name_completions(types));
        }
        return Some(Vec::new());
    }

    // Check if cursor is inside a field's arguments list
    if let Some(items) = try_argument_completions(db, project_files, block_context.tree, offset) {
        return Some(items);
    }

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
                field_completions(db, project_files, block_context.tree, types, offset)
            } else {
                Some(keyword_completions())
            }
        }
        _ => Some(Vec::new()),
    }
}

/// Try to provide completions when the cursor is inside a field's arguments.
///
/// Handles two cases:
/// 1. Cursor at argument name position -> suggest argument names
/// 2. Cursor at argument value position (after `:`) -> suggest enum values if applicable
///
/// Returns `Some(items)` if the cursor is in an arguments context, `None` otherwise.
fn try_argument_completions(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    project_files: Option<graphql_base_db::ProjectFiles>,
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
) -> Option<Vec<CompletionItem>> {
    let project_files = project_files?;
    let arg_ctx = find_argument_context_at_offset(tree, offset)?;

    let types = graphql_hir::schema_types(db, project_files);
    let parent_ctx = find_parent_type_at_offset(tree, offset)?;
    let parent_type_name =
        crate::symbol::walk_type_stack_to_offset(tree, types, offset, &parent_ctx.root_type)?;

    let parent_type = types.get(parent_type_name.as_str())?;
    let field_def = parent_type
        .fields
        .iter()
        .find(|f| f.name.as_ref() == arg_ctx.field_name)?;

    // If we're in an argument value position, try type-aware completions
    if let Some(arg_name) = &arg_ctx.argument_name {
        if let Some(arg_def) = field_def
            .arguments
            .iter()
            .find(|a| a.name.as_ref() == arg_name)
        {
            let base_type_name = arg_def.type_ref.name.as_ref();
            if let Some(type_def) = types.get(base_type_name) {
                if type_def.kind == graphql_hir::TypeDefKind::Enum {
                    return Some(enum_value_completions(type_def));
                }
                if type_def.kind == graphql_hir::TypeDefKind::InputObject {
                    return Some(input_field_completions(type_def));
                }
            }
        }
        // In value position but not an enum/input type - return empty to avoid showing arg names
        return Some(Vec::new());
    }

    // Cursor is at argument name position - suggest argument names
    let items = field_def
        .arguments
        .iter()
        .map(|arg| {
            let mut item = CompletionItem::new(arg.name.to_string(), CompletionKind::Argument)
                .with_detail(format_type_ref(&arg.type_ref));
            if let Some(desc) = &arg.description {
                item = item.with_documentation(desc.to_string());
            }
            if arg.is_deprecated {
                item = item.with_deprecated(true);
            }
            // Insert "argName: " to make it easy to type the value
            item = item.with_insert_text(format!("{}: ", arg.name));
            item
        })
        .collect();

    Some(items)
}

/// Generate completion items for input object fields.
fn input_field_completions(type_def: &graphql_hir::TypeDef) -> Vec<CompletionItem> {
    type_def
        .fields
        .iter()
        .map(|field| {
            let mut item = CompletionItem::new(field.name.to_string(), CompletionKind::Field)
                .with_detail(format_type_ref(&field.type_ref));
            if let Some(desc) = &field.description {
                item = item.with_documentation(desc.to_string());
            }
            if field.is_deprecated {
                item = item.with_deprecated(true);
            }
            // Insert "fieldName: " for quick value entry
            item = item.with_insert_text(format!("{}: ", field.name));
            item
        })
        .collect()
}

/// Generate completion items for enum values.
fn enum_value_completions(type_def: &graphql_hir::TypeDef) -> Vec<CompletionItem> {
    type_def
        .enum_values
        .iter()
        .map(|ev| {
            let mut item = CompletionItem::new(ev.name.to_string(), CompletionKind::EnumValue);
            if let Some(desc) = &ev.description {
                item = item.with_documentation(desc.to_string());
            }
            if ev.is_deprecated {
                item = item.with_deprecated(true);
            }
            item
        })
        .collect()
}

/// Check if the cursor immediately follows an `@` sign.
fn is_after_at_sign(source: &str, offset: usize) -> bool {
    if offset == 0 {
        return false;
    }
    source.as_bytes().get(offset - 1) == Some(&b'@')
}

/// Check if the cursor immediately follows a `$` sign.
fn is_after_dollar_sign(source: &str, offset: usize) -> bool {
    if offset == 0 {
        return false;
    }
    source.as_bytes().get(offset - 1) == Some(&b'$')
}

/// Generate completion items for variables defined on the current operation.
fn variable_completions(tree: &apollo_parser::SyntaxTree, offset: usize) -> Vec<CompletionItem> {
    let Some(variables) = find_operation_variables_at_offset(tree, offset) else {
        return Vec::new();
    };

    variables
        .into_iter()
        .map(|(name, type_str)| {
            CompletionItem::new(name, CompletionKind::Variable).with_detail(type_str)
        })
        .collect()
}

/// Generate completion items for directives.
fn directive_completions() -> Vec<CompletionItem> {
    BUILTIN_DIRECTIVES
        .iter()
        .map(|(name, description, locations)| {
            CompletionItem::new(name.to_string(), CompletionKind::Directive)
                .with_detail(locations.to_string())
                .with_documentation(description.to_string())
        })
        .collect()
}

/// Check if the cursor is in a type name position.
///
/// Returns true if the cursor follows:
/// - `on ` (fragment/inline fragment type condition)
/// - `: ` in a variable definition context
fn is_in_type_position(source: &str, offset: usize) -> bool {
    let before = source.get(..offset).unwrap_or("");
    let trimmed = before.trim_end();
    // Check for `on` keyword (fragment type condition or inline fragment)
    if trimmed.ends_with(" on") || trimmed.ends_with("\ton") || trimmed.ends_with("\non") {
        return true;
    }
    // Also check if trimmed itself is just "on" (start of line)
    if trimmed == "on" {
        return true;
    }
    false
}

/// Generate completion items for type names from the schema.
fn type_name_completions(types: &graphql_hir::TypeDefMap) -> Vec<CompletionItem> {
    types
        .values()
        .filter(|t| {
            // Only suggest types that can appear in type positions
            // Exclude InputObject types for fragment type conditions
            matches!(
                t.kind,
                graphql_hir::TypeDefKind::Object
                    | graphql_hir::TypeDefKind::Interface
                    | graphql_hir::TypeDefKind::Union
            )
        })
        .map(|t| {
            let kind_label = match t.kind {
                graphql_hir::TypeDefKind::Object => "object",
                graphql_hir::TypeDefKind::Interface => "interface",
                graphql_hir::TypeDefKind::Union => "union",
                _ => "type",
            };
            let mut item = CompletionItem::new(t.name.to_string(), CompletionKind::Type)
                .with_detail(kind_label.to_string());
            if let Some(desc) = &t.description {
                item = item.with_documentation(desc.to_string());
            }
            item
        })
        .collect()
}

/// Generate completion items for top-level GraphQL keywords.
fn keyword_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem::new("query".to_string(), CompletionKind::Keyword)
            .with_detail("Define a query operation".to_string())
            .with_insert_text("query $1 {\n  $0\n}".to_string())
            .with_insert_text_format(InsertTextFormat::Snippet),
        CompletionItem::new("mutation".to_string(), CompletionKind::Keyword)
            .with_detail("Define a mutation operation".to_string())
            .with_insert_text("mutation $1 {\n  $0\n}".to_string())
            .with_insert_text_format(InsertTextFormat::Snippet),
        CompletionItem::new("subscription".to_string(), CompletionKind::Keyword)
            .with_detail("Define a subscription operation".to_string())
            .with_insert_text("subscription $1 {\n  $0\n}".to_string())
            .with_insert_text_format(InsertTextFormat::Snippet),
        CompletionItem::new("fragment".to_string(), CompletionKind::Keyword)
            .with_detail("Define a fragment".to_string())
            .with_insert_text("fragment $1 on $2 {\n  $0\n}".to_string())
            .with_insert_text_format(InsertTextFormat::Snippet),
    ]
}

/// Provide field completions in a selection set.
fn field_completions(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
    tree: &apollo_parser::SyntaxTree,
    types: &graphql_hir::TypeDefMap,
    offset: usize,
) -> Option<Vec<CompletionItem>> {
    let parent_ctx = find_parent_type_at_offset(tree, offset)?;
    let parent_type_name =
        crate::symbol::walk_type_stack_to_offset(tree, types, offset, &parent_ctx.root_type)?;

    types.get(parent_type_name.as_str()).map_or_else(
        || Some(Vec::new()),
        |parent_type| {
            if parent_type.kind == graphql_hir::TypeDefKind::Union {
                let items: Vec<CompletionItem> = parent_type
                    .union_members
                    .iter()
                    .map(|member| {
                        CompletionItem::new(format!("... on {member}"), CompletionKind::Type)
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
                let implementors = graphql_hir::interface_implementors(db, project_files);
                if let Some(impl_types) = implementors.get(&parent_type.name) {
                    for type_name in impl_types {
                        let inline_fragment_label = format!("... on {type_name}");
                        items.push(
                            CompletionItem::new(inline_fragment_label, CompletionKind::Type)
                                .with_insert_text(format!("... on {type_name} {{\n  $0\n}}"))
                                .with_insert_text_format(InsertTextFormat::Snippet)
                                .with_sort_text(format!("z_{type_name}")),
                        );
                    }
                }
            }
            Some(items)
        },
    )
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
