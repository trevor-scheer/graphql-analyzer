//! Completion feature implementation.
//!
//! This module provides IDE auto-completion functionality including:
//! - Field completions in selection sets
//! - Fragment spread completions
//! - Inline fragment completions for unions and interfaces
//! - Argument completions for fields and directives
//! - Variable completions
//! - Directive completions
//! - Type completions (for variable definitions)
//! - Enum value completions

use crate::helpers::{find_block_for_position, format_type_ref, position_to_offset};
use crate::symbol::{
    find_completion_context, find_parent_type_at_offset, find_symbol_at_offset,
    is_in_selection_set, CompletionContext, DirectiveLocation, Symbol,
};
use crate::types::{CompletionItem, CompletionKind, FilePath, InsertTextFormat, Position};
use crate::FileRegistry;
use graphql_hir::TypeDefKind;
use std::collections::HashMap;
use std::sync::Arc;

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

    // Get schema types if available (owned HashMap, not reference)
    let schema_types = project_files.map(|pf| graphql_hir::schema_types(db, pf).clone());

    // First, try to determine completion context using the new system
    let completion_ctx =
        find_completion_context(block_context.block_source, block_context.tree, offset);

    match completion_ctx {
        Some(CompletionContext::Variable) => {
            // Complete with operation-defined variables
            complete_variables(block_context.tree, offset)
        }
        Some(CompletionContext::Directive { location }) => {
            // Complete with available directives
            complete_directives(location)
        }
        Some(CompletionContext::TypeName { input_only }) => {
            // Complete with type names
            complete_types(schema_types.as_ref(), input_only)
        }
        Some(CompletionContext::Argument {
            field_name,
            directive_name,
            parent_type,
        }) => {
            // Complete with field or directive arguments
            complete_arguments(
                block_context.tree,
                offset,
                schema_types.as_ref(),
                field_name.as_deref(),
                directive_name.as_deref(),
                parent_type.as_deref(),
            )
        }
        Some(CompletionContext::EnumValue { enum_type }) => {
            // Complete with enum values
            complete_enum_values(schema_types.as_ref(), &enum_type)
        }
        Some(CompletionContext::FragmentSpread) | None => {
            // Fall back to the original logic
            complete_field_or_fragment(db, project_files, block_context.tree, offset, schema_types)
        }
        Some(CompletionContext::Field { .. }) => {
            // Field completions
            complete_field_or_fragment(db, project_files, block_context.tree, offset, schema_types)
        }
        Some(CompletionContext::InlineFragmentType { parent_type }) => {
            // Complete types for inline fragments
            complete_inline_fragment_types(schema_types.as_ref(), parent_type.as_deref())
        }
    }
}

/// Complete variables defined in the current operation
#[allow(clippy::unnecessary_wraps)]
fn complete_variables(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<Vec<CompletionItem>> {
    use apollo_parser::cst::{self, CstNode};

    let doc = tree.document();

    // Find the operation that contains this offset
    for definition in doc.definitions() {
        if let cst::Definition::OperationDefinition(op) = definition {
            let op_range = op.syntax().text_range();
            let op_start: usize = op_range.start().into();
            let op_end: usize = op_range.end().into();

            if byte_offset >= op_start && byte_offset <= op_end {
                // This is our operation, get its variables
                let mut items = Vec::new();

                if let Some(var_defs) = op.variable_definitions() {
                    for var_def in var_defs.variable_definitions() {
                        if let Some(var) = var_def.variable() {
                            if let Some(name) = var.name() {
                                let var_name = name.text().to_string();

                                // Get type info if available
                                let type_str = var_def.ty().map(|t| format_cst_type(&t));

                                let mut item =
                                    CompletionItem::new(var_name.clone(), CompletionKind::Variable);

                                if let Some(t) = type_str {
                                    item = item.with_detail(t);
                                }

                                items.push(item);
                            }
                        }
                    }
                }

                return Some(items);
            }
        }
    }

    Some(Vec::new())
}

/// Format a CST Type node to a string
fn format_cst_type(ty: &apollo_parser::cst::Type) -> String {
    use apollo_parser::cst;

    match ty {
        cst::Type::NamedType(named) => named
            .name()
            .map_or_else(String::new, |n| n.text().to_string()),
        cst::Type::ListType(list) => {
            let inner = list.ty().map_or_else(String::new, |t| format_cst_type(&t));
            format!("[{inner}]")
        }
        cst::Type::NonNullType(non_null) => {
            if let Some(named) = non_null.named_type() {
                let name = named
                    .name()
                    .map_or_else(String::new, |n| n.text().to_string());
                format!("{name}!")
            } else if let Some(list) = non_null.list_type() {
                let inner = list.ty().map_or_else(String::new, |t| format_cst_type(&t));
                format!("[{inner}]!")
            } else {
                String::new()
            }
        }
    }
}

/// Complete with available directives based on location
#[allow(clippy::unnecessary_wraps)]
fn complete_directives(location: DirectiveLocation) -> Option<Vec<CompletionItem>> {
    let mut items = Vec::new();

    // Built-in directives that are always available
    let builtin_directives = [
        ("skip", "if: Boolean!", "Directs the executor to skip this field or fragment when the `if` argument is true."),
        ("include", "if: Boolean!", "Directs the executor to include this field or fragment only when the `if` argument is true."),
        ("deprecated", "reason: String", "Marks an element of a GraphQL schema as no longer supported."),
    ];

    // Filter directives by location
    for (name, args, description) in &builtin_directives {
        // @skip and @include are valid on FIELD, FRAGMENT_SPREAD, and INLINE_FRAGMENT
        // @deprecated is for schema definitions, not queries
        let is_valid_location = matches!(
            (*name, location),
            (
                "skip" | "include",
                DirectiveLocation::Field
                    | DirectiveLocation::FragmentSpread
                    | DirectiveLocation::InlineFragment
            )
        );

        if is_valid_location {
            let insert_text = if args.is_empty() {
                (*name).to_string()
            } else {
                format!("{name}($1)")
            };

            items.push(
                CompletionItem::new(*name, CompletionKind::Directive)
                    .with_detail(format!("@{name}({args})"))
                    .with_documentation(*description)
                    .with_insert_text(insert_text)
                    .with_insert_text_format(InsertTextFormat::Snippet),
            );
        }
    }

    Some(items)
}

/// Complete with type names from the schema
#[allow(clippy::unnecessary_wraps)]
fn complete_types(
    schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>>,
    input_only: bool,
) -> Option<Vec<CompletionItem>> {
    let Some(types) = schema_types else {
        return Some(Vec::new());
    };

    let mut items = Vec::new();

    // Add built-in scalar types
    let builtins = ["String", "Int", "Float", "Boolean", "ID"];
    for name in builtins {
        items.push(CompletionItem::new(name, CompletionKind::Type).with_detail("Built-in scalar"));
    }

    // Add schema types
    for (name, type_def) in types {
        let is_valid = if input_only {
            // For variable definitions, only input types, scalars, and enums are valid
            matches!(
                type_def.kind,
                TypeDefKind::InputObject | TypeDefKind::Scalar | TypeDefKind::Enum
            )
        } else {
            true
        };

        if is_valid {
            let kind_str = match type_def.kind {
                TypeDefKind::Interface => "interface",
                TypeDefKind::Union => "union",
                TypeDefKind::Enum => "enum",
                TypeDefKind::Scalar => "scalar",
                TypeDefKind::InputObject => "input",
                _ => "type", // Object and future type kinds
            };

            let mut item = CompletionItem::new(name.to_string(), CompletionKind::Type)
                .with_detail(kind_str.to_string());

            if let Some(desc) = &type_def.description {
                item = item.with_documentation(desc.to_string());
            }

            items.push(item);
        }
    }

    Some(items)
}

/// Complete arguments for a field or directive
#[allow(clippy::too_many_arguments, clippy::unnecessary_wraps)]
fn complete_arguments(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
    schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>>,
    field_name: Option<&str>,
    directive_name: Option<&str>,
    _parent_type_hint: Option<&str>,
) -> Option<Vec<CompletionItem>> {
    let mut items = Vec::new();

    // Handle directive arguments
    if let Some(dir_name) = directive_name {
        match dir_name {
            "skip" | "include" => {
                items.push(
                    CompletionItem::new("if", CompletionKind::Argument)
                        .with_detail("Boolean!")
                        .with_insert_text("if: $1")
                        .with_insert_text_format(InsertTextFormat::Snippet),
                );
            }
            "deprecated" => {
                items.push(
                    CompletionItem::new("reason", CompletionKind::Argument)
                        .with_detail("String")
                        .with_insert_text("reason: \"$1\"")
                        .with_insert_text_format(InsertTextFormat::Snippet),
                );
            }
            _ => {}
        }
        return Some(items);
    }

    // Handle field arguments - need to find the parent type first
    if let Some(f_name) = field_name {
        if let Some(types) = schema_types {
            // Find the parent type for this field
            let parent_type_name = find_parent_type_for_field(tree, byte_offset, types);

            if let Some(parent_name) = parent_type_name {
                if let Some(parent_type) = types.get(parent_name.as_str()) {
                    // Find the field
                    if let Some(field) = parent_type
                        .fields
                        .iter()
                        .find(|f| f.name.as_ref() == f_name)
                    {
                        // Add argument completions
                        for arg in &field.arguments {
                            let type_str = format_type_ref(&arg.type_ref);
                            let mut item =
                                CompletionItem::new(arg.name.to_string(), CompletionKind::Argument)
                                    .with_detail(type_str.clone());

                            if let Some(desc) = &arg.description {
                                item = item.with_documentation(desc.to_string());
                            }

                            // Create appropriate insert text based on type
                            let insert_text = if arg.type_ref.name.as_ref() == "String" {
                                format!("{}: \"$1\"", arg.name)
                            } else if arg.type_ref.name.as_ref() == "Boolean" {
                                format!("{}: ${{1:true}}", arg.name)
                            } else {
                                format!("{}: $1", arg.name)
                            };

                            item = item
                                .with_insert_text(insert_text)
                                .with_insert_text_format(InsertTextFormat::Snippet);

                            if arg.is_deprecated {
                                item = item.with_deprecated(true);
                            }

                            items.push(item);
                        }
                    }
                }
            }
        }
    }

    Some(items)
}

/// Find the parent type name for a field at the given offset
fn find_parent_type_for_field(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
) -> Option<String> {
    let parent_ctx = find_parent_type_at_offset(tree, byte_offset)?;
    crate::symbol::walk_type_stack_to_offset(tree, schema_types, byte_offset, &parent_ctx.root_type)
}

/// Complete enum values for a specific enum type
#[allow(clippy::unnecessary_wraps)]
fn complete_enum_values(
    schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>>,
    enum_type: &str,
) -> Option<Vec<CompletionItem>> {
    let Some(types) = schema_types else {
        return Some(Vec::new());
    };

    let Some(type_def) = types.get(enum_type) else {
        return Some(Vec::new());
    };

    if type_def.kind != TypeDefKind::Enum {
        return Some(Vec::new());
    }

    let items: Vec<CompletionItem> = type_def
        .enum_values
        .iter()
        .map(|ev| {
            let mut item = CompletionItem::new(ev.name.to_string(), CompletionKind::EnumValue)
                .with_detail(enum_type.to_string());

            if let Some(desc) = &ev.description {
                item = item.with_documentation(desc.to_string());
            }

            if ev.is_deprecated {
                item = item.with_deprecated(true);
            }

            item
        })
        .collect();

    Some(items)
}

/// Complete types for inline fragment type conditions
#[allow(clippy::unnecessary_wraps)]
fn complete_inline_fragment_types(
    schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>>,
    parent_type: Option<&str>,
) -> Option<Vec<CompletionItem>> {
    let Some(types) = schema_types else {
        return Some(Vec::new());
    };

    let mut items = Vec::new();

    // If we know the parent type, only show valid subtypes
    if let Some(parent_name) = parent_type {
        if let Some(parent_def) = types.get(parent_name) {
            match parent_def.kind {
                TypeDefKind::Union => {
                    // Show union members
                    for member in &parent_def.union_members {
                        items.push(
                            CompletionItem::new(member.to_string(), CompletionKind::Type)
                                .with_detail("type".to_string())
                                .with_insert_text(format!("{member} {{\n  $0\n}}"))
                                .with_insert_text_format(InsertTextFormat::Snippet),
                        );
                    }
                }
                TypeDefKind::Interface => {
                    // Show implementing types
                    for type_def in types.values() {
                        if type_def.implements.contains(&parent_def.name) {
                            items.push(
                                CompletionItem::new(
                                    type_def.name.to_string(),
                                    CompletionKind::Type,
                                )
                                .with_detail("type".to_string())
                                .with_insert_text(format!("{} {{\n  $0\n}}", type_def.name))
                                .with_insert_text_format(InsertTextFormat::Snippet),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    } else {
        // Show all object and interface types
        for (name, type_def) in types {
            if matches!(type_def.kind, TypeDefKind::Object | TypeDefKind::Interface) {
                items.push(
                    CompletionItem::new(name.to_string(), CompletionKind::Type)
                        .with_detail(if type_def.kind == TypeDefKind::Interface {
                            "interface"
                        } else {
                            "type"
                        })
                        .with_insert_text(format!("{name} {{\n  $0\n}}"))
                        .with_insert_text_format(InsertTextFormat::Snippet),
                );
            }
        }
    }

    Some(items)
}

/// Complete fields or fragments (original logic)
#[allow(clippy::too_many_lines)]
fn complete_field_or_fragment(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    project_files: Option<graphql_base_db::ProjectFiles>,
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
    schema_types: Option<HashMap<Arc<str>, graphql_hir::TypeDef>>,
) -> Option<Vec<CompletionItem>> {
    let symbol = find_symbol_at_offset(tree, offset);

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
            let Some(types) = schema_types else {
                return Some(Vec::new());
            };

            let in_selection_set = is_in_selection_set(tree, offset);
            if in_selection_set {
                let parent_ctx = find_parent_type_at_offset(tree, offset)?;
                let parent_type_name = crate::symbol::walk_type_stack_to_offset(
                    tree,
                    &types,
                    offset,
                    &parent_ctx.root_type,
                )?;

                types.get(parent_type_name.as_str()).map_or_else(
                    || Some(Vec::new()),
                    |parent_type| {
                        if parent_type.kind == TypeDefKind::Union {
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
                                let mut item = CompletionItem::new(
                                    field.name.to_string(),
                                    CompletionKind::Field,
                                )
                                .with_detail(format_type_ref(&field.type_ref));

                                if let Some(desc) = &field.description {
                                    item = item.with_documentation(desc.to_string());
                                }

                                if field.is_deprecated {
                                    item = item.with_deprecated(true);
                                }

                                // If field has required arguments, add them to insert text
                                let required_args: Vec<_> = field
                                    .arguments
                                    .iter()
                                    .filter(|a| a.type_ref.is_non_null && a.default_value.is_none())
                                    .collect();

                                if !required_args.is_empty() {
                                    let args_snippet: Vec<String> = required_args
                                        .iter()
                                        .enumerate()
                                        .map(|(i, arg)| format!("{}: ${}", arg.name, i + 1))
                                        .collect();
                                    let insert =
                                        format!("{}({})", field.name, args_snippet.join(", "));
                                    item = item
                                        .with_insert_text(insert)
                                        .with_insert_text_format(InsertTextFormat::Snippet);
                                }

                                item
                            })
                            .collect();

                        // Add __typename field
                        items.push(
                            CompletionItem::new("__typename", CompletionKind::Field)
                                .with_detail("String!")
                                .with_documentation(
                                    "The name of the current Object type at runtime.",
                                ),
                        );

                        if parent_type.kind == TypeDefKind::Interface {
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
    use crate::symbol::find_completion_context;
    use apollo_parser::Parser;

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

    #[test]
    fn test_variable_completion_context() {
        let source = "query GetUser($";
        let parser = Parser::new(source);
        let tree = parser.parse();

        let ctx = find_completion_context(source, &tree, source.len());
        assert_eq!(ctx, Some(CompletionContext::Variable));
    }

    #[test]
    fn test_directive_completion_context() {
        let source = "query { user @";
        let parser = Parser::new(source);
        let tree = parser.parse();

        let ctx = find_completion_context(source, &tree, source.len());
        assert!(matches!(ctx, Some(CompletionContext::Directive { .. })));
    }

    #[test]
    fn test_argument_completion_context() {
        let source = "query { user(";
        let parser = Parser::new(source);
        let tree = parser.parse();

        let ctx = find_completion_context(source, &tree, source.len());
        assert!(matches!(ctx, Some(CompletionContext::Argument { .. })));
    }

    #[test]
    fn test_field_completion_context() {
        // Use a more complete query that can be parsed properly
        let source = "query { user { name } }";
        let parser = Parser::new(source);
        let tree = parser.parse();

        // Position at "name" (index 15)
        let ctx = find_completion_context(source, &tree, 15);
        // In a valid selection set, when we're at a field position, we get Field context
        assert!(
            matches!(ctx, Some(CompletionContext::Field { .. })),
            "Expected Field context, got {ctx:?}",
        );
    }

    #[test]
    fn test_type_completion_in_variable() {
        // Use a complete query with a variable type
        let source = "query GetUser($id: ID!) { user }";
        let parser = Parser::new(source);
        let tree = parser.parse();

        // Position after the colon in $id: (index 19, where "ID!" starts)
        let ctx = find_completion_context(source, &tree, 19);
        assert!(
            matches!(ctx, Some(CompletionContext::TypeName { input_only: true })),
            "Expected TypeName context with input_only: true, got {ctx:?}",
        );
    }

    #[test]
    fn test_complete_variables_returns_defined_vars() {
        let source = "query GetUser($userId: ID!, $active: Boolean) { user(id: $";
        let parser = Parser::new(source);
        let tree = parser.parse();

        let items = complete_variables(&tree, source.len()).unwrap();
        assert_eq!(items.len(), 2);

        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"userId"));
        assert!(names.contains(&"active"));
    }

    #[test]
    fn test_complete_directives_for_field() {
        let items = complete_directives(DirectiveLocation::Field).unwrap();

        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"skip"));
        assert!(names.contains(&"include"));
        assert!(!names.contains(&"deprecated")); // deprecated is for schema, not queries
    }
}
