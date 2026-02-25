/// Walks the CST from the root to the cursor, maintaining a stack of type context.
/// Returns the type at the cursor position for completions, following the field chain and fragments.
pub fn walk_type_stack_to_offset(
    tree: &apollo_parser::SyntaxTree,
    schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
    byte_offset: usize,
    root_type: &str,
) -> Option<String> {
    fn walk_selection_set(
        selection_set: &cst::SelectionSet,
        offset: usize,
        schema_types: &std::collections::HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
        type_stack: &mut Vec<String>,
        found: &mut bool,
        entered: &mut bool,
    ) {
        let start: usize = selection_set.syntax().text_range().start().into();
        let end: usize = selection_set.syntax().text_range().end().into();
        if offset < start || offset > end {
            return;
        }
        // Mark that we've entered the selection set containing the cursor
        *entered = true;
        for selection in selection_set.selections() {
            if let cst::Selection::Field(field) = selection {
                if let Some(nested) = field.selection_set() {
                    let nstart: usize = nested.syntax().text_range().start().into();
                    let nend: usize = nested.syntax().text_range().end().into();
                    if offset >= nstart && offset <= nend {
                        // Descend into this field's selection set
                        if let Some(field_name) = field.name().map(|n| n.text().to_string()) {
                            if let Some(parent_type) = type_stack.last().cloned() {
                                if let Some(type_def) = schema_types.get(parent_type.as_str()) {
                                    if let Some(field_def) = type_def
                                        .fields
                                        .iter()
                                        .find(|f| f.name.as_ref() == field_name)
                                    {
                                        let field_type =
                                            crate::unwrap_type_to_name(&field_def.type_ref);
                                        tracing::debug!(
                                            "PUSH field {} type {}",
                                            field_name,
                                            field_type
                                        );
                                        type_stack.push(field_type);
                                        walk_selection_set(
                                            &nested,
                                            offset,
                                            schema_types,
                                            type_stack,
                                            found,
                                            entered,
                                        );
                                        if *found {
                                            return;
                                        }
                                        type_stack.pop();
                                    }
                                }
                            }
                        }
                        // Once we descend, don't mark this set as the cursor's set
                        *entered = false;
                    }
                }
            } else if let cst::Selection::InlineFragment(inline_frag) = selection {
                if let Some(nested) = inline_frag.selection_set() {
                    let nstart: usize = nested.syntax().text_range().start().into();
                    let nend: usize = nested.syntax().text_range().end().into();
                    if offset >= nstart && offset <= nend {
                        if let Some(type_cond) = inline_frag.type_condition() {
                            if let Some(named_type) = type_cond.named_type() {
                                if let Some(name) = named_type.name() {
                                    tracing::debug!("PUSH inline fragment type {}", name.text());
                                    type_stack.push(name.text().to_string());
                                    walk_selection_set(
                                        &nested,
                                        offset,
                                        schema_types,
                                        type_stack,
                                        found,
                                        entered,
                                    );
                                    if *found {
                                        return;
                                    }
                                    type_stack.pop();
                                }
                            }
                        } else {
                            walk_selection_set(
                                &nested,
                                offset,
                                schema_types,
                                type_stack,
                                found,
                                entered,
                            );
                            if *found {
                                return;
                            }
                        }
                        *entered = false;
                    }
                }
            }
        }
        // If we reach here and entered is true, this is the selection set at the cursor
        if *entered {
            *found = true;
        }
    }
    // (Removed duplicate unreachable code block)

    let doc = tree.document();
    let mut type_stack = vec![root_type.to_string()];
    let mut found = false;
    let mut entered = false;
    for definition in doc.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    let start: usize = selection_set.syntax().text_range().start().into();
                    let end: usize = selection_set.syntax().text_range().end().into();
                    if byte_offset >= start && byte_offset <= end {
                        walk_selection_set(
                            &selection_set,
                            byte_offset,
                            schema_types,
                            &mut type_stack,
                            &mut found,
                            &mut entered,
                        );
                        break;
                    }
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    let start: usize = selection_set.syntax().text_range().start().into();
                    let end: usize = selection_set.syntax().text_range().end().into();
                    if byte_offset >= start && byte_offset <= end {
                        // Use fragment type condition as root
                        if let Some(type_cond) = frag.type_condition() {
                            if let Some(named_type) = type_cond.named_type() {
                                if let Some(name) = named_type.name() {
                                    type_stack[0] = name.text().to_string();
                                }
                            }
                        }
                        walk_selection_set(
                            &selection_set,
                            byte_offset,
                            schema_types,
                            &mut type_stack,
                            &mut found,
                            &mut entered,
                        );
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    type_stack.last().cloned()
}

// Symbol identification at positions
//
// This module provides utilities for finding GraphQL symbols at specific positions
// in source code, using apollo-parser's CST for position lookups.

use apollo_parser::cst::{self, CstNode};

/// A GraphQL symbol identified at a specific position
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Variants may be added for future completion contexts
pub enum Symbol {
    /// A type name reference (in type positions, implements clauses, etc.)
    TypeName { name: String },
    /// A field selection in a query/fragment
    FieldName { name: String },
    /// A fragment spread (...`FragmentName`)
    FragmentSpread { name: String },
    /// An operation name
    OperationName { name: String },
    /// A variable reference ($varName)
    VariableReference { name: String },
    /// An argument name in a field or directive
    ArgumentName { name: String },
    /// A directive reference (@directiveName)
    DirectiveName { name: String },
    /// An enum value
    EnumValue { name: String },
}

/// Context for completion at a specific position
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Some variants are for future use or testing
pub enum CompletionContext {
    /// Completing field names in a selection set
    Field { parent_type: Option<String> },
    /// Completing fragment spreads after `...`
    FragmentSpread,
    /// Completing arguments for a field or directive
    Argument {
        field_name: Option<String>,
        directive_name: Option<String>,
        parent_type: Option<String>,
    },
    /// Completing variable names after `$`
    Variable,
    /// Completing directive names after `@`
    Directive {
        /// Location where directive is being used
        location: DirectiveLocation,
    },
    /// Completing type names (in variable definitions, type conditions, etc.)
    TypeName {
        /// Whether only input types are valid (for variable definitions)
        input_only: bool,
    },
    /// Completing enum values in a value position
    EnumValue { enum_type: String },
    /// Completing inline fragment type conditions after `... on`
    InlineFragmentType { parent_type: Option<String> },
}

/// Locations where directives can appear
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Some variants are for future use or location-specific directive filtering
pub enum DirectiveLocation {
    /// On a query operation
    Query,
    /// On a mutation operation
    Mutation,
    /// On a subscription operation
    Subscription,
    /// On a field selection
    Field,
    /// On a fragment definition
    FragmentDefinition,
    /// On a fragment spread
    FragmentSpread,
    /// On an inline fragment
    InlineFragment,
    /// On a variable definition
    VariableDefinition,
    /// Unknown location
    Unknown,
}

/// Find the symbol at a specific byte offset in the document
pub fn find_symbol_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<Symbol> {
    let doc = tree.document();

    for definition in doc.definitions() {
        if let Some(symbol) = check_definition(&definition, byte_offset) {
            return Some(symbol);
        }
    }

    None
}

/// Context about the parent type at a position
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentTypeContext {
    /// The root type (Query, Mutation, Subscription, or fragment's type condition)
    pub root_type: String,
    /// The immediate parent (field name or type name at the cursor position)
    pub immediate_parent: String,
}

/// Find the parent type name at a given position in a selection set.
/// Returns both the root type and the immediate parent context.
pub fn find_parent_type_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<ParentTypeContext> {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    let start: usize = selection_set.syntax().text_range().start().into();
                    let end: usize = selection_set.syntax().text_range().end().into();

                    if byte_offset >= start && byte_offset <= end {
                        let root_type = match op.operation_type() {
                            Some(op_type) if op_type.mutation_token().is_some() => "Mutation",
                            Some(op_type) if op_type.subscription_token().is_some() => {
                                "Subscription"
                            }
                            _ => "Query", // Default to Query for explicit query or shorthand
                        };

                        let immediate_parent = find_parent_field_type(&selection_set, byte_offset)
                            .unwrap_or_else(|| root_type.to_string());

                        return Some(ParentTypeContext {
                            root_type: root_type.to_string(),
                            immediate_parent,
                        });
                    }
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    let start: usize = selection_set.syntax().text_range().start().into();
                    let end: usize = selection_set.syntax().text_range().end().into();

                    if byte_offset >= start && byte_offset <= end {
                        // For fragments, the parent type is the type condition
                        if let Some(type_cond) = frag.type_condition() {
                            if let Some(named_type) = type_cond.named_type() {
                                if let Some(name) = named_type.name() {
                                    let root_type = name.text().to_string();
                                    // Try to find nested parent, otherwise use fragment's type
                                    let immediate_parent =
                                        find_parent_field_type(&selection_set, byte_offset)
                                            .unwrap_or_else(|| root_type.clone());

                                    return Some(ParentTypeContext {
                                        root_type,
                                        immediate_parent,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Find the parent field's type name within a selection set
/// Returns a vector of field names from outermost to innermost (e.g., `["pokemon", "evolution"]`)
fn find_parent_field_path(
    selection_set: &cst::SelectionSet,
    byte_offset: usize,
) -> Option<Vec<String>> {
    let mut best_path: Option<Vec<String>> = None;
    for selection in selection_set.selections() {
        if let cst::Selection::Field(field) = selection {
            if let Some(nested) = field.selection_set() {
                let start: usize = nested.syntax().text_range().start().into();
                let end: usize = nested.syntax().text_range().end().into();
                if byte_offset >= start && byte_offset <= end {
                    // Cursor is inside the nested selection set, so add this field to the path
                    let field_name = field.name()?.text().to_string();
                    // The path should be [field_name] + path inside nested
                    if let Some(mut deeper_path) = find_parent_field_path(&nested, byte_offset) {
                        let mut path = vec![field_name];
                        path.append(&mut deeper_path);
                        if best_path.as_ref().is_none_or(|p| path.len() > p.len()) {
                            best_path = Some(path);
                        }
                    } else {
                        // If we're directly in the selection set, the path is just this field
                        let path = vec![field_name];
                        if best_path.as_ref().is_none_or(|p| path.len() > p.len()) {
                            best_path = Some(path);
                        }
                    }
                }
            }
        } else if let cst::Selection::InlineFragment(inline_frag) = selection {
            if let Some(nested) = inline_frag.selection_set() {
                let start: usize = nested.syntax().text_range().start().into();
                let end: usize = nested.syntax().text_range().end().into();
                if byte_offset >= start && byte_offset <= end {
                    // For inline fragments, recurse but do NOT add to the path (fragments don't add a field)
                    if let Some(deeper_path) = find_parent_field_path(&nested, byte_offset) {
                        if best_path
                            .as_ref()
                            .is_none_or(|p| deeper_path.len() > p.len())
                        {
                            best_path = Some(deeper_path);
                        }
                    } else if best_path.is_none() {
                        best_path = Some(vec![]);
                    }
                }
            }
        }
    }
    // If the cursor is directly in this selection set (not in any nested set), return empty path
    if best_path.is_none() {
        // But only if the offset is actually inside this selection set
        let start: usize = selection_set.syntax().text_range().start().into();
        let end: usize = selection_set.syntax().text_range().end().into();
        if byte_offset >= start && byte_offset <= end {
            return Some(vec![]);
        }
    }
    best_path
}

/// Find the parent field's type name within a selection set (legacy wrapper)
fn find_parent_field_type(selection_set: &cst::SelectionSet, byte_offset: usize) -> Option<String> {
    find_parent_field_path(selection_set, byte_offset).and_then(|path| path.last().cloned())
}

/// Find the parent type name for a field at the given offset in a schema definition.
/// Returns the name of the type (object, interface, or input) that contains the field.
pub fn find_schema_field_parent_type(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<String> {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(obj) => {
                if let Some(fields) = obj.fields_definition() {
                    let range = fields.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    if byte_offset >= start && byte_offset <= end {
                        return obj.name().map(|n| n.text().to_string());
                    }
                }
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                if let Some(fields) = iface.fields_definition() {
                    let range = fields.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    if byte_offset >= start && byte_offset <= end {
                        return iface.name().map(|n| n.text().to_string());
                    }
                }
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                if let Some(fields) = input.input_fields_definition() {
                    let range = fields.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    if byte_offset >= start && byte_offset <= end {
                        return input.name().map(|n| n.text().to_string());
                    }
                }
            }
            // Type extensions
            cst::Definition::ObjectTypeExtension(ext) => {
                if let Some(fields) = ext.fields_definition() {
                    let range = fields.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    if byte_offset >= start && byte_offset <= end {
                        return ext.name().map(|n| n.text().to_string());
                    }
                }
            }
            cst::Definition::InterfaceTypeExtension(ext) => {
                if let Some(fields) = ext.fields_definition() {
                    let range = fields.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    if byte_offset >= start && byte_offset <= end {
                        return ext.name().map(|n| n.text().to_string());
                    }
                }
            }
            cst::Definition::InputObjectTypeExtension(ext) => {
                if let Some(fields) = ext.input_fields_definition() {
                    let range = fields.syntax().text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();
                    if byte_offset >= start && byte_offset <= end {
                        return ext.name().map(|n| n.text().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Check if the byte offset is within a selection set (for field completions)
pub fn is_in_selection_set(tree: &apollo_parser::SyntaxTree, byte_offset: usize) -> bool {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    if is_offset_in_selection_set(&selection_set, byte_offset) {
                        return true;
                    }
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    if is_offset_in_selection_set(&selection_set, byte_offset) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }

    false
}

fn is_offset_in_selection_set(selection_set: &cst::SelectionSet, byte_offset: usize) -> bool {
    // Check if offset is within the selection set's range
    let start = selection_set.syntax().text_range().start().into();
    let end = selection_set.syntax().text_range().end().into();

    if byte_offset < start || byte_offset > end {
        return false;
    }

    // We're within the selection set's range, but check if we're in a nested selection set
    for selection in selection_set.selections() {
        if let cst::Selection::Field(field) = selection {
            if let Some(nested) = field.selection_set() {
                if is_offset_in_selection_set(&nested, byte_offset) {
                    return true;
                }
            }
        } else if let cst::Selection::InlineFragment(inline_frag) = selection {
            if let Some(nested) = inline_frag.selection_set() {
                if is_offset_in_selection_set(&nested, byte_offset) {
                    return true;
                }
            }
        }
    }

    // We're in this selection set (not in a nested one)
    true
}

fn check_definition(definition: &cst::Definition, byte_offset: usize) -> Option<Symbol> {
    match definition {
        cst::Definition::OperationDefinition(op) => check_operation(op, byte_offset),
        cst::Definition::FragmentDefinition(frag) => check_fragment_definition(frag, byte_offset),
        cst::Definition::ObjectTypeDefinition(obj) => {
            check_type_definition_name(obj.name(), byte_offset)
                .or_else(|| check_implements_interfaces(obj.implements_interfaces(), byte_offset))
                .or_else(|| check_fields_definition(obj.fields_definition(), byte_offset))
        }
        cst::Definition::InterfaceTypeDefinition(iface) => {
            check_type_definition_name(iface.name(), byte_offset)
                .or_else(|| check_implements_interfaces(iface.implements_interfaces(), byte_offset))
                .or_else(|| check_fields_definition(iface.fields_definition(), byte_offset))
        }
        cst::Definition::ObjectTypeExtension(ext) => {
            check_type_definition_name(ext.name(), byte_offset)
                .or_else(|| check_implements_interfaces(ext.implements_interfaces(), byte_offset))
                .or_else(|| check_fields_definition(ext.fields_definition(), byte_offset))
        }
        cst::Definition::InterfaceTypeExtension(ext) => {
            check_type_definition_name(ext.name(), byte_offset)
                .or_else(|| check_implements_interfaces(ext.implements_interfaces(), byte_offset))
                .or_else(|| check_fields_definition(ext.fields_definition(), byte_offset))
        }
        cst::Definition::UnionTypeDefinition(union) => {
            check_type_definition_name(union.name(), byte_offset).or_else(|| {
                if let Some(members) = union.union_member_types() {
                    for member in members.named_types() {
                        if let Some(name) = member.name() {
                            if is_within_range(&name, byte_offset) {
                                return Some(Symbol::TypeName {
                                    name: name.text().to_string(),
                                });
                            }
                        }
                    }
                }
                None
            })
        }
        cst::Definition::EnumTypeDefinition(enum_def) => {
            check_type_definition_name(enum_def.name(), byte_offset)
        }
        cst::Definition::ScalarTypeDefinition(scalar) => {
            check_type_definition_name(scalar.name(), byte_offset)
        }
        cst::Definition::ScalarTypeExtension(ext) => {
            check_type_definition_name(ext.name(), byte_offset)
        }
        cst::Definition::InputObjectTypeDefinition(input) => {
            check_type_definition_name(input.name(), byte_offset).or_else(|| {
                check_input_fields_definition(input.input_fields_definition(), byte_offset)
            })
        }
        _ => None,
    }
}

fn check_fields_definition(
    fields: Option<cst::FieldsDefinition>,
    byte_offset: usize,
) -> Option<Symbol> {
    let fields = fields?;
    for field in fields.field_definitions() {
        if let Some(name) = field.name() {
            if is_within_range(&name, byte_offset) {
                return Some(Symbol::FieldName {
                    name: name.text().to_string(),
                });
            }
        }
        if let Some(ty) = field.ty() {
            if let Some(symbol) = check_type_reference(&ty, byte_offset) {
                return Some(symbol);
            }
        }
        if let Some(args) = field.arguments_definition() {
            for arg in args.input_value_definitions() {
                if let Some(ty) = arg.ty() {
                    if let Some(symbol) = check_type_reference(&ty, byte_offset) {
                        return Some(symbol);
                    }
                }
            }
        }
    }
    None
}

fn check_input_fields_definition(
    fields: Option<cst::InputFieldsDefinition>,
    byte_offset: usize,
) -> Option<Symbol> {
    let fields = fields?;
    for field in fields.input_value_definitions() {
        if let Some(ty) = field.ty() {
            if let Some(symbol) = check_type_reference(&ty, byte_offset) {
                return Some(symbol);
            }
        }
    }
    None
}

fn check_type_reference(ty: &cst::Type, byte_offset: usize) -> Option<Symbol> {
    match ty {
        cst::Type::NamedType(named) => {
            if let Some(name) = named.name() {
                if is_within_range(&name, byte_offset) {
                    return Some(Symbol::TypeName {
                        name: name.text().to_string(),
                    });
                }
            }
        }
        cst::Type::ListType(list) => {
            if let Some(inner) = list.ty() {
                return check_type_reference(&inner, byte_offset);
            }
        }
        cst::Type::NonNullType(non_null) => {
            if let Some(named) = non_null.named_type() {
                if let Some(name) = named.name() {
                    if is_within_range(&name, byte_offset) {
                        return Some(Symbol::TypeName {
                            name: name.text().to_string(),
                        });
                    }
                }
            }
            if let Some(list) = non_null.list_type() {
                if let Some(inner) = list.ty() {
                    return check_type_reference(&inner, byte_offset);
                }
            }
        }
    }
    None
}

fn check_type_definition_name(name: Option<cst::Name>, byte_offset: usize) -> Option<Symbol> {
    if let Some(name) = name {
        if is_within_range(&name, byte_offset) {
            return Some(Symbol::TypeName {
                name: name.text().to_string(),
            });
        }
    }
    None
}

fn check_implements_interfaces(
    implements: Option<cst::ImplementsInterfaces>,
    byte_offset: usize,
) -> Option<Symbol> {
    let implements = implements?;
    for named_type in implements.named_types() {
        if let Some(name) = named_type.name() {
            if is_within_range(&name, byte_offset) {
                return Some(Symbol::TypeName {
                    name: name.text().to_string(),
                });
            }
        }
    }
    None
}

fn check_operation(op: &cst::OperationDefinition, byte_offset: usize) -> Option<Symbol> {
    if let Some(name) = op.name() {
        if is_within_range(&name, byte_offset) {
            return Some(Symbol::OperationName {
                name: name.text().to_string(),
            });
        }
    }

    if let Some(selection_set) = op.selection_set() {
        if let Some(symbol) = check_selection_set(&selection_set, byte_offset) {
            return Some(symbol);
        }
    }

    None
}

fn check_fragment_definition(frag: &cst::FragmentDefinition, byte_offset: usize) -> Option<Symbol> {
    if let Some(frag_name) = frag.fragment_name() {
        if let Some(name) = frag_name.name() {
            if is_within_range(&name, byte_offset) {
                return Some(Symbol::FragmentSpread {
                    name: name.text().to_string(),
                });
            }
        }
    }

    if let Some(type_cond) = frag.type_condition() {
        if let Some(named_type) = type_cond.named_type() {
            if let Some(name) = named_type.name() {
                if is_within_range(&name, byte_offset) {
                    return Some(Symbol::TypeName {
                        name: name.text().to_string(),
                    });
                }
            }
        }
    }

    if let Some(selection_set) = frag.selection_set() {
        if let Some(symbol) = check_selection_set(&selection_set, byte_offset) {
            return Some(symbol);
        }
    }

    None
}

fn check_arguments(arguments: &cst::Arguments, byte_offset: usize) -> Option<Symbol> {
    for arg in arguments.arguments() {
        if let Some(name) = arg.name() {
            if is_within_range(&name, byte_offset) {
                return Some(Symbol::ArgumentName {
                    name: name.text().to_string(),
                });
            }
        }
        // Check argument value for variable references
        if let Some(value) = arg.value() {
            if let Some(symbol) = check_value(&value, byte_offset) {
                return Some(symbol);
            }
        }
    }
    None
}

fn check_value(value: &cst::Value, byte_offset: usize) -> Option<Symbol> {
    match value {
        cst::Value::Variable(var) => {
            if is_within_range(var, byte_offset) {
                // Extract variable name (without the $ prefix)
                let name = var.name()?.text().to_string();
                return Some(Symbol::VariableReference { name });
            }
        }
        cst::Value::ListValue(list) => {
            for val in list.values() {
                if let Some(symbol) = check_value(&val, byte_offset) {
                    return Some(symbol);
                }
            }
        }
        cst::Value::ObjectValue(obj) => {
            for field in obj.object_fields() {
                if let Some(val) = field.value() {
                    if let Some(symbol) = check_value(&val, byte_offset) {
                        return Some(symbol);
                    }
                }
            }
        }
        // Other value types don't contain symbols we care about
        _ => {}
    }
    None
}

fn check_selection_set(selection_set: &cst::SelectionSet, byte_offset: usize) -> Option<Symbol> {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(name) = field.name() {
                    if is_within_range(&name, byte_offset) {
                        return Some(Symbol::FieldName {
                            name: name.text().to_string(),
                        });
                    }
                }

                if let Some(arguments) = field.arguments() {
                    if let Some(symbol) = check_arguments(&arguments, byte_offset) {
                        return Some(symbol);
                    }
                }

                if let Some(nested) = field.selection_set() {
                    if let Some(symbol) = check_selection_set(&nested, byte_offset) {
                        return Some(symbol);
                    }
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(name) = spread.fragment_name().and_then(|n| n.name()) {
                    if is_within_range(&name, byte_offset) {
                        return Some(Symbol::FragmentSpread {
                            name: name.text().to_string(),
                        });
                    }
                }
            }
            cst::Selection::InlineFragment(inline_frag) => {
                if let Some(type_cond) = inline_frag.type_condition() {
                    if let Some(named_type) = type_cond.named_type() {
                        if let Some(name) = named_type.name() {
                            if is_within_range(&name, byte_offset) {
                                return Some(Symbol::TypeName {
                                    name: name.text().to_string(),
                                });
                            }
                        }
                    }
                }

                if let Some(nested) = inline_frag.selection_set() {
                    if let Some(symbol) = check_selection_set(&nested, byte_offset) {
                        return Some(symbol);
                    }
                }
            }
        }
    }

    None
}

/// Check if a byte offset falls within the range of a CST node
fn is_within_range<T: CstNode>(node: &T, byte_offset: usize) -> bool {
    let range = node.syntax().text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    byte_offset >= start && byte_offset < end
}

/// Find the byte offset range of a fragment definition by name
pub fn find_fragment_definition_range(
    tree: &apollo_parser::SyntaxTree,
    fragment_name: &str,
) -> Option<(usize, usize)> {
    let doc = tree.document();

    for definition in doc.definitions() {
        if let cst::Definition::FragmentDefinition(frag) = definition {
            if let Some(name) = frag.fragment_name().and_then(|n| n.name()) {
                if name.text() == fragment_name {
                    let range = name.syntax().text_range();
                    return Some((range.start().into(), range.end().into()));
                }
            }
        }
    }

    None
}

/// Find the byte offset range of a type definition by name
pub fn find_type_definition_range(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
) -> Option<(usize, usize)> {
    let doc = tree.document();

    for definition in doc.definitions() {
        let name_node = match definition {
            cst::Definition::ObjectTypeDefinition(obj) => obj.name(),
            cst::Definition::InterfaceTypeDefinition(iface) => iface.name(),
            cst::Definition::UnionTypeDefinition(union) => union.name(),
            cst::Definition::EnumTypeDefinition(enum_def) => enum_def.name(),
            cst::Definition::ScalarTypeDefinition(scalar) => scalar.name(),
            cst::Definition::InputObjectTypeDefinition(input) => input.name(),
            // Type extensions
            cst::Definition::ObjectTypeExtension(ext) => ext.name(),
            cst::Definition::InterfaceTypeExtension(ext) => ext.name(),
            cst::Definition::UnionTypeExtension(ext) => ext.name(),
            cst::Definition::EnumTypeExtension(ext) => ext.name(),
            cst::Definition::InputObjectTypeExtension(ext) => ext.name(),
            cst::Definition::ScalarTypeExtension(ext) => ext.name(),
            _ => None,
        };

        if let Some(name) = name_node {
            if name.text() == type_name {
                let range = name.syntax().text_range();
                return Some((range.start().into(), range.end().into()));
            }
        }
    }

    None
}

/// Find all fragment spreads with a specific name in a syntax tree
pub fn find_fragment_spreads(
    tree: &apollo_parser::SyntaxTree,
    fragment_name: &str,
) -> Option<Vec<usize>> {
    let mut offsets = Vec::new();
    let doc = tree.document();

    for definition in doc.definitions() {
        if let cst::Definition::OperationDefinition(op) = definition {
            if let Some(selection_set) = op.selection_set() {
                find_fragment_spreads_in_selection_set(&selection_set, fragment_name, &mut offsets);
            }
        } else if let cst::Definition::FragmentDefinition(frag) = definition {
            if let Some(selection_set) = frag.selection_set() {
                find_fragment_spreads_in_selection_set(&selection_set, fragment_name, &mut offsets);
            }
        }
    }

    if offsets.is_empty() {
        None
    } else {
        Some(offsets)
    }
}

fn find_fragment_spreads_in_selection_set(
    selection_set: &cst::SelectionSet,
    fragment_name: &str,
    offsets: &mut Vec<usize>,
) {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(nested) = field.selection_set() {
                    find_fragment_spreads_in_selection_set(&nested, fragment_name, offsets);
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(name) = spread.fragment_name().and_then(|n| n.name()) {
                    if name.text() == fragment_name {
                        let range = name.syntax().text_range();
                        offsets.push(range.start().into());
                    }
                }
            }
            cst::Selection::InlineFragment(inline_frag) => {
                if let Some(nested) = inline_frag.selection_set() {
                    find_fragment_spreads_in_selection_set(&nested, fragment_name, offsets);
                }
            }
        }
    }
}

/// Find all references to a type name in a syntax tree
pub fn find_type_references_in_tree(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
) -> Option<Vec<usize>> {
    let mut offsets = Vec::new();
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(obj) => {
                find_type_refs_in_object_type(&obj, type_name, &mut offsets);
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                find_type_refs_in_interface_type(&iface, type_name, &mut offsets);
            }
            cst::Definition::UnionTypeDefinition(union) => {
                find_type_refs_in_union_type(&union, type_name, &mut offsets);
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                find_type_refs_in_input_object_type(&input, type_name, &mut offsets);
            }
            cst::Definition::ObjectTypeExtension(ext) => {
                find_type_refs_in_object_type_ext(&ext, type_name, &mut offsets);
            }
            cst::Definition::InterfaceTypeExtension(ext) => {
                find_type_refs_in_interface_type_ext(&ext, type_name, &mut offsets);
            }
            cst::Definition::UnionTypeExtension(ext) => {
                find_type_refs_in_union_type_ext(&ext, type_name, &mut offsets);
            }
            cst::Definition::InputObjectTypeExtension(ext) => {
                find_type_refs_in_input_object_type_ext(&ext, type_name, &mut offsets);
            }
            _ => {}
        }
    }

    if offsets.is_empty() {
        None
    } else {
        Some(offsets)
    }
}

fn find_type_refs_in_object_type(
    obj: &cst::ObjectTypeDefinition,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    if let Some(implements) = obj.implements_interfaces() {
        for iface in implements.named_types() {
            if let Some(name) = iface.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
    }

    for field in obj
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        if let Some(args) = field.arguments_definition() {
            for arg in args.input_value_definitions() {
                if let Some(ty) = arg.ty() {
                    find_type_refs_in_type(&ty, type_name, offsets);
                }
            }
        }
    }
}

fn find_type_refs_in_interface_type(
    iface: &cst::InterfaceTypeDefinition,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    if let Some(implements) = iface.implements_interfaces() {
        for impl_iface in implements.named_types() {
            if let Some(name) = impl_iface.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
    }

    for field in iface
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        if let Some(args) = field.arguments_definition() {
            for arg in args.input_value_definitions() {
                if let Some(ty) = arg.ty() {
                    find_type_refs_in_type(&ty, type_name, offsets);
                }
            }
        }
    }
}

fn find_type_refs_in_union_type(
    union: &cst::UnionTypeDefinition,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    if let Some(members) = union.union_member_types() {
        for member in members.named_types() {
            if let Some(name) = member.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
    }
}

fn find_type_refs_in_input_object_type(
    input: &cst::InputObjectTypeDefinition,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    for field in input
        .input_fields_definition()
        .into_iter()
        .flat_map(|f| f.input_value_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }
    }
}

fn find_type_refs_in_object_type_ext(
    ext: &cst::ObjectTypeExtension,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    if let Some(implements) = ext.implements_interfaces() {
        for iface in implements.named_types() {
            if let Some(name) = iface.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
    }

    for field in ext
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        if let Some(args) = field.arguments_definition() {
            for arg in args.input_value_definitions() {
                if let Some(ty) = arg.ty() {
                    find_type_refs_in_type(&ty, type_name, offsets);
                }
            }
        }
    }
}

fn find_type_refs_in_interface_type_ext(
    ext: &cst::InterfaceTypeExtension,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    if let Some(implements) = ext.implements_interfaces() {
        for iface in implements.named_types() {
            if let Some(name) = iface.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
    }

    for field in ext
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        if let Some(args) = field.arguments_definition() {
            for arg in args.input_value_definitions() {
                if let Some(ty) = arg.ty() {
                    find_type_refs_in_type(&ty, type_name, offsets);
                }
            }
        }
    }
}

fn find_type_refs_in_union_type_ext(
    ext: &cst::UnionTypeExtension,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    if let Some(members) = ext.union_member_types() {
        for member in members.named_types() {
            if let Some(name) = member.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
    }
}

fn find_type_refs_in_input_object_type_ext(
    ext: &cst::InputObjectTypeExtension,
    type_name: &str,
    offsets: &mut Vec<usize>,
) {
    for field in ext
        .input_fields_definition()
        .into_iter()
        .flat_map(|f| f.input_value_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }
    }
}

fn find_type_refs_in_type(ty: &cst::Type, type_name: &str, offsets: &mut Vec<usize>) {
    match ty {
        cst::Type::NamedType(named) => {
            if let Some(name) = named.name() {
                if name.text() == type_name {
                    let range = name.syntax().text_range();
                    offsets.push(range.start().into());
                }
            }
        }
        cst::Type::ListType(list) => {
            if let Some(inner) = list.ty() {
                find_type_refs_in_type(&inner, type_name, offsets);
            }
        }
        cst::Type::NonNullType(non_null) => {
            // NonNullType contains either a NamedType or a ListType
            if let Some(named) = non_null.named_type() {
                if let Some(name) = named.name() {
                    if name.text() == type_name {
                        let range = name.syntax().text_range();
                        offsets.push(range.start().into());
                    }
                }
            } else if let Some(list) = non_null.list_type() {
                if let Some(inner) = list.ty() {
                    find_type_refs_in_type(&inner, type_name, offsets);
                }
            }
        }
    }
}

/// Symbol range info containing both name range and full definition range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolRanges {
    /// Start byte offset of the name
    pub name_start: usize,
    /// End byte offset of the name
    pub name_end: usize,
    /// Start byte offset of the full definition
    pub def_start: usize,
    /// End byte offset of the full definition
    pub def_end: usize,
}

/// Find the byte offset ranges of a type definition by name
/// Returns both name range (for selection) and full definition range
pub fn find_type_definition_full_range(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
) -> Option<SymbolRanges> {
    let doc = tree.document();

    for definition in doc.definitions() {
        let (name_node, def_syntax) = match &definition {
            cst::Definition::ObjectTypeDefinition(obj) => (obj.name(), obj.syntax()),
            cst::Definition::InterfaceTypeDefinition(iface) => (iface.name(), iface.syntax()),
            cst::Definition::UnionTypeDefinition(union) => (union.name(), union.syntax()),
            cst::Definition::EnumTypeDefinition(enum_def) => (enum_def.name(), enum_def.syntax()),
            cst::Definition::ScalarTypeDefinition(scalar) => (scalar.name(), scalar.syntax()),
            cst::Definition::InputObjectTypeDefinition(input) => (input.name(), input.syntax()),
            // Type extensions
            cst::Definition::ObjectTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::InterfaceTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::UnionTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::EnumTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::InputObjectTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::ScalarTypeExtension(ext) => (ext.name(), ext.syntax()),
            _ => continue,
        };

        if let Some(name) = name_node {
            if name.text() == type_name {
                let name_range = name.syntax().text_range();
                let def_range = def_syntax.text_range();
                return Some(SymbolRanges {
                    name_start: name_range.start().into(),
                    name_end: name_range.end().into(),
                    def_start: def_range.start().into(),
                    def_end: def_range.end().into(),
                });
            }
        }
    }

    None
}

/// Find ALL type definitions and extensions matching a name in a single tree.
/// Returns all matches (base types and extensions) for multi-location goto-def.
pub fn find_all_type_definitions_full_range(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
) -> Vec<SymbolRanges> {
    let doc = tree.document();
    let mut results = Vec::new();

    for definition in doc.definitions() {
        let (name_node, def_syntax) = match &definition {
            cst::Definition::ObjectTypeDefinition(obj) => (obj.name(), obj.syntax()),
            cst::Definition::InterfaceTypeDefinition(iface) => (iface.name(), iface.syntax()),
            cst::Definition::UnionTypeDefinition(union) => (union.name(), union.syntax()),
            cst::Definition::EnumTypeDefinition(enum_def) => (enum_def.name(), enum_def.syntax()),
            cst::Definition::ScalarTypeDefinition(scalar) => (scalar.name(), scalar.syntax()),
            cst::Definition::InputObjectTypeDefinition(input) => (input.name(), input.syntax()),
            cst::Definition::ObjectTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::InterfaceTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::UnionTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::EnumTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::InputObjectTypeExtension(ext) => (ext.name(), ext.syntax()),
            cst::Definition::ScalarTypeExtension(ext) => (ext.name(), ext.syntax()),
            _ => continue,
        };

        if let Some(name) = name_node {
            if name.text() == type_name {
                let name_range = name.syntax().text_range();
                let def_range = def_syntax.text_range();
                results.push(SymbolRanges {
                    name_start: name_range.start().into(),
                    name_end: name_range.end().into(),
                    def_start: def_range.start().into(),
                    def_end: def_range.end().into(),
                });
            }
        }
    }

    results
}

/// Find the byte offset ranges of a fragment definition by name
/// Returns both name range (for selection) and full definition range
pub fn find_fragment_definition_full_range(
    tree: &apollo_parser::SyntaxTree,
    fragment_name: &str,
) -> Option<SymbolRanges> {
    let doc = tree.document();

    for definition in doc.definitions() {
        if let cst::Definition::FragmentDefinition(frag) = definition {
            if let Some(name) = frag.fragment_name().and_then(|n| n.name()) {
                if name.text() == fragment_name {
                    let name_range = name.syntax().text_range();
                    let def_range = frag.syntax().text_range();
                    return Some(SymbolRanges {
                        name_start: name_range.start().into(),
                        name_end: name_range.end().into(),
                        def_start: def_range.start().into(),
                        def_end: def_range.end().into(),
                    });
                }
            }
        }
    }

    None
}

/// Find the byte offset ranges of an operation definition by name
/// Returns both name range (for selection) and full definition range
/// Returns None for anonymous operations when searching by name
pub fn find_operation_definition_ranges(
    tree: &apollo_parser::SyntaxTree,
    operation_name: &str,
) -> Option<SymbolRanges> {
    let doc = tree.document();

    for definition in doc.definitions() {
        if let cst::Definition::OperationDefinition(op) = definition {
            if let Some(name) = op.name() {
                if name.text() == operation_name {
                    let name_range = name.syntax().text_range();
                    let def_range = op.syntax().text_range();
                    return Some(SymbolRanges {
                        name_start: name_range.start().into(),
                        name_end: name_range.end().into(),
                        def_start: def_range.start().into(),
                        def_end: def_range.end().into(),
                    });
                }
            }
        }
    }

    None
}

/// Find the byte offset ranges of a field definition within a type
/// Returns both name range (for selection) and full field definition range
pub fn find_field_definition_full_range(
    tree: &apollo_parser::SyntaxTree,
    type_name: &str,
    field_name: &str,
) -> Option<SymbolRanges> {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(obj) => {
                if obj.name().is_some_and(|n| n.text() == type_name) {
                    if let Some(fields) = obj.fields_definition() {
                        for field in fields.field_definitions() {
                            if let Some(name) = field.name() {
                                if name.text() == field_name {
                                    let name_range = name.syntax().text_range();
                                    let def_range = field.syntax().text_range();
                                    return Some(SymbolRanges {
                                        name_start: name_range.start().into(),
                                        name_end: name_range.end().into(),
                                        def_start: def_range.start().into(),
                                        def_end: def_range.end().into(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                if iface.name().is_some_and(|n| n.text() == type_name) {
                    if let Some(fields) = iface.fields_definition() {
                        for field in fields.field_definitions() {
                            if let Some(name) = field.name() {
                                if name.text() == field_name {
                                    let name_range = name.syntax().text_range();
                                    let def_range = field.syntax().text_range();
                                    return Some(SymbolRanges {
                                        name_start: name_range.start().into(),
                                        name_end: name_range.end().into(),
                                        def_start: def_range.start().into(),
                                        def_end: def_range.end().into(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                if input.name().is_some_and(|n| n.text() == type_name) {
                    if let Some(fields) = input.input_fields_definition() {
                        for field in fields.input_value_definitions() {
                            if let Some(name) = field.name() {
                                if name.text() == field_name {
                                    let name_range = name.syntax().text_range();
                                    let def_range = field.syntax().text_range();
                                    return Some(SymbolRanges {
                                        name_start: name_range.start().into(),
                                        name_end: name_range.end().into(),
                                        def_start: def_range.start().into(),
                                        def_end: def_range.end().into(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // Type extensions - fields can also be defined in extend type declarations
            cst::Definition::ObjectTypeExtension(ext) => {
                if ext.name().is_some_and(|n| n.text() == type_name) {
                    if let Some(fields) = ext.fields_definition() {
                        for field in fields.field_definitions() {
                            if let Some(name) = field.name() {
                                if name.text() == field_name {
                                    let name_range = name.syntax().text_range();
                                    let def_range = field.syntax().text_range();
                                    return Some(SymbolRanges {
                                        name_start: name_range.start().into(),
                                        name_end: name_range.end().into(),
                                        def_start: def_range.start().into(),
                                        def_end: def_range.end().into(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cst::Definition::InterfaceTypeExtension(ext) => {
                if ext.name().is_some_and(|n| n.text() == type_name) {
                    if let Some(fields) = ext.fields_definition() {
                        for field in fields.field_definitions() {
                            if let Some(name) = field.name() {
                                if name.text() == field_name {
                                    let name_range = name.syntax().text_range();
                                    let def_range = field.syntax().text_range();
                                    return Some(SymbolRanges {
                                        name_start: name_range.start().into(),
                                        name_end: name_range.end().into(),
                                        def_start: def_range.start().into(),
                                        def_end: def_range.end().into(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cst::Definition::InputObjectTypeExtension(ext) => {
                if ext.name().is_some_and(|n| n.text() == type_name) {
                    if let Some(fields) = ext.input_fields_definition() {
                        for field in fields.input_value_definitions() {
                            if let Some(name) = field.name() {
                                if name.text() == field_name {
                                    let name_range = name.syntax().text_range();
                                    let def_range = field.syntax().text_range();
                                    return Some(SymbolRanges {
                                        name_start: name_range.start().into(),
                                        name_end: name_range.end().into(),
                                        def_start: def_range.start().into(),
                                        def_end: def_range.end().into(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract all definitions from a document for document symbols
/// Returns a list of (name, kind, ranges) for each definition
pub fn extract_all_definitions(
    tree: &apollo_parser::SyntaxTree,
) -> Vec<(String, &'static str, SymbolRanges)> {
    let doc = tree.document();
    let mut results = Vec::new();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(obj) => {
                if let Some(name) = obj.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = obj.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "object",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                if let Some(name) = iface.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = iface.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "interface",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::UnionTypeDefinition(union) => {
                if let Some(name) = union.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = union.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "union",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::EnumTypeDefinition(enum_def) => {
                if let Some(name) = enum_def.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = enum_def.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "enum",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::ScalarTypeDefinition(scalar) => {
                if let Some(name) = scalar.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = scalar.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "scalar",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                if let Some(name) = input.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = input.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "input",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::OperationDefinition(op) => {
                let name = op
                    .name()
                    .map_or_else(|| "<anonymous>".to_string(), |n| n.text().to_string());
                let kind = op.operation_type().map_or("query", |op_type| {
                    if op_type.mutation_token().is_some() {
                        "mutation"
                    } else if op_type.subscription_token().is_some() {
                        "subscription"
                    } else {
                        "query"
                    }
                });

                let def_range = op.syntax().text_range();
                let name_range = op.name().map_or(def_range, |n| n.syntax().text_range());

                results.push((
                    name,
                    kind,
                    SymbolRanges {
                        name_start: name_range.start().into(),
                        name_end: name_range.end().into(),
                        def_start: def_range.start().into(),
                        def_end: def_range.end().into(),
                    },
                ));
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(name) = frag.fragment_name().and_then(|n| n.name()) {
                    let name_range = name.syntax().text_range();
                    let def_range = frag.syntax().text_range();
                    results.push((
                        name.text().to_string(),
                        "fragment",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            // Type extensions
            cst::Definition::ObjectTypeExtension(ext) => {
                if let Some(name) = ext.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = ext.syntax().text_range();
                    results.push((
                        format!("extend type {}", name.text()),
                        "object",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::InterfaceTypeExtension(ext) => {
                if let Some(name) = ext.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = ext.syntax().text_range();
                    results.push((
                        format!("extend interface {}", name.text()),
                        "interface",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::UnionTypeExtension(ext) => {
                if let Some(name) = ext.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = ext.syntax().text_range();
                    results.push((
                        format!("extend union {}", name.text()),
                        "union",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::EnumTypeExtension(ext) => {
                if let Some(name) = ext.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = ext.syntax().text_range();
                    results.push((
                        format!("extend enum {}", name.text()),
                        "enum",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::InputObjectTypeExtension(ext) => {
                if let Some(name) = ext.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = ext.syntax().text_range();
                    results.push((
                        format!("extend input {}", name.text()),
                        "input",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            cst::Definition::ScalarTypeExtension(ext) => {
                if let Some(name) = ext.name() {
                    let name_range = name.syntax().text_range();
                    let def_range = ext.syntax().text_range();
                    results.push((
                        format!("extend scalar {}", name.text()),
                        "scalar",
                        SymbolRanges {
                            name_start: name_range.start().into(),
                            name_end: name_range.end().into(),
                            def_start: def_range.start().into(),
                            def_end: def_range.end().into(),
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    results
}

/// Find the completion context at a specific byte offset.
/// This analyzes the source text and position to determine what kind of completions are appropriate.
pub fn find_completion_context(
    source: &str,
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<CompletionContext> {
    // Get the characters before the cursor for context
    let prefix = if byte_offset > 0 && byte_offset <= source.len() {
        &source[..byte_offset]
    } else {
        ""
    };

    // Check for $ prefix (variable completion)
    if let Some(last_non_ws) = prefix.trim_end().chars().last() {
        if last_non_ws == '$' {
            return Some(CompletionContext::Variable);
        }
    }

    // Check if we just typed $ as the last character
    if prefix.ends_with('$') {
        return Some(CompletionContext::Variable);
    }

    // Check for @ prefix (directive completion)
    if prefix.ends_with('@') {
        let location = find_directive_location(tree, byte_offset);
        return Some(CompletionContext::Directive { location });
    }

    // Check for : in variable definition (type completion)
    if is_in_variable_type_position(source, tree, byte_offset) {
        return Some(CompletionContext::TypeName { input_only: true });
    }

    // Check for argument position (after ( or in arguments)
    if let Some(ctx) = find_argument_context(tree, byte_offset) {
        return Some(ctx);
    }

    // Check if we're in an enum value position
    if let Some(enum_type) = find_enum_value_context(tree, byte_offset) {
        return Some(CompletionContext::EnumValue { enum_type });
    }

    // Default: check for field/fragment context based on symbol
    let symbol = find_symbol_at_offset(tree, byte_offset);
    match symbol {
        Some(Symbol::FragmentSpread { .. }) => Some(CompletionContext::FragmentSpread),
        Some(Symbol::DirectiveName { .. }) => {
            let location = find_directive_location(tree, byte_offset);
            Some(CompletionContext::Directive { location })
        }
        Some(Symbol::FieldName { .. }) | None => {
            if is_in_selection_set(tree, byte_offset) {
                Some(CompletionContext::Field { parent_type: None })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Find the directive location for a given position
fn find_directive_location(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> DirectiveLocation {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                let def_range = op.syntax().text_range();
                let def_start: usize = def_range.start().into();
                let def_end: usize = def_range.end().into();
                if byte_offset >= def_start && byte_offset <= def_end {
                    // Check if we're on a variable definition
                    if let Some(var_defs) = op.variable_definitions() {
                        let var_range = var_defs.syntax().text_range();
                        let var_start: usize = var_range.start().into();
                        let var_end: usize = var_range.end().into();
                        if byte_offset >= var_start && byte_offset <= var_end {
                            return DirectiveLocation::VariableDefinition;
                        }
                    }

                    // Check if we're in a selection set (field directive)
                    if let Some(selection_set) = op.selection_set() {
                        let sel_range = selection_set.syntax().text_range();
                        let sel_start: usize = sel_range.start().into();
                        let sel_end: usize = sel_range.end().into();
                        if byte_offset >= sel_start && byte_offset <= sel_end {
                            return DirectiveLocation::Field;
                        }
                    }

                    // Otherwise it's on the operation itself
                    return match op.operation_type() {
                        Some(op_type) if op_type.mutation_token().is_some() => {
                            DirectiveLocation::Mutation
                        }
                        Some(op_type) if op_type.subscription_token().is_some() => {
                            DirectiveLocation::Subscription
                        }
                        _ => DirectiveLocation::Query,
                    };
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                let def_range = frag.syntax().text_range();
                let def_start: usize = def_range.start().into();
                let def_end: usize = def_range.end().into();
                if byte_offset >= def_start && byte_offset <= def_end {
                    // Check if we're in a selection set (field directive)
                    if let Some(selection_set) = frag.selection_set() {
                        let sel_range = selection_set.syntax().text_range();
                        let sel_start: usize = sel_range.start().into();
                        let sel_end: usize = sel_range.end().into();
                        if byte_offset >= sel_start && byte_offset <= sel_end {
                            return DirectiveLocation::Field;
                        }
                    }
                    return DirectiveLocation::FragmentDefinition;
                }
            }
            _ => {}
        }
    }

    DirectiveLocation::Unknown
}

/// Check if the cursor is in a variable type position (after : in variable definition)
fn is_in_variable_type_position(
    source: &str,
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> bool {
    let doc = tree.document();

    for definition in doc.definitions() {
        if let cst::Definition::OperationDefinition(op) = definition {
            if let Some(var_defs) = op.variable_definitions() {
                let var_range = var_defs.syntax().text_range();
                let var_start: usize = var_range.start().into();
                let var_end: usize = var_range.end().into();

                if byte_offset >= var_start && byte_offset <= var_end {
                    // We're inside variable definitions
                    // Check if there's a : before the cursor (in this variable context)
                    let prefix = &source[var_start..byte_offset.min(source.len())];
                    // Find the last variable start ($) and check if there's a : after it
                    if let Some(last_dollar) = prefix.rfind('$') {
                        let after_dollar = &prefix[last_dollar..];
                        if after_dollar.contains(':') {
                            // There's a : after the $, so we're in type position
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Find argument context if cursor is in an argument position
fn find_argument_context(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<CompletionContext> {
    let doc = tree.document();

    for definition in doc.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    if let Some(ctx) = find_argument_in_selection_set(&selection_set, byte_offset) {
                        return Some(ctx);
                    }
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    if let Some(ctx) = find_argument_in_selection_set(&selection_set, byte_offset) {
                        return Some(ctx);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

fn find_argument_in_selection_set(
    selection_set: &cst::SelectionSet,
    byte_offset: usize,
) -> Option<CompletionContext> {
    for selection in selection_set.selections() {
        if let cst::Selection::Field(field) = selection {
            // Check if we're in this field's arguments
            if let Some(arguments) = field.arguments() {
                let arg_range = arguments.syntax().text_range();
                let arg_start: usize = arg_range.start().into();
                let arg_end: usize = arg_range.end().into();

                if byte_offset >= arg_start && byte_offset <= arg_end {
                    let field_name = field.name().map(|n| n.text().to_string());
                    return Some(CompletionContext::Argument {
                        field_name,
                        directive_name: None,
                        parent_type: None,
                    });
                }
            }

            // Check directives on the field
            if let Some(directives) = field.directives() {
                for directive in directives.directives() {
                    if let Some(arguments) = directive.arguments() {
                        let arg_range = arguments.syntax().text_range();
                        let arg_start: usize = arg_range.start().into();
                        let arg_end: usize = arg_range.end().into();

                        if byte_offset >= arg_start && byte_offset <= arg_end {
                            let directive_name = directive.name().map(|n| n.text().to_string());
                            return Some(CompletionContext::Argument {
                                field_name: None,
                                directive_name,
                                parent_type: None,
                            });
                        }
                    }
                }
            }

            // Recurse into nested selection sets
            if let Some(nested) = field.selection_set() {
                if let Some(ctx) = find_argument_in_selection_set(&nested, byte_offset) {
                    return Some(ctx);
                }
            }
        } else if let cst::Selection::InlineFragment(inline_frag) = selection {
            if let Some(nested) = inline_frag.selection_set() {
                if let Some(ctx) = find_argument_in_selection_set(&nested, byte_offset) {
                    return Some(ctx);
                }
            }
        }
    }

    None
}

/// Find enum value context if cursor is in an argument value position expecting an enum
fn find_enum_value_context(
    _tree: &apollo_parser::SyntaxTree,
    _byte_offset: usize,
) -> Option<String> {
    // This requires knowing the argument type from the schema
    // The actual enum type resolution will happen in completion.rs
    // where we have access to the schema types
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_parser::Parser;

    #[test]
    fn test_find_field_name() {
        let source = "query { user { name } }";
        let parser = Parser::new(source);
        let tree = parser.parse();

        // Position at "user"
        let symbol = find_symbol_at_offset(&tree, 8);
        assert_eq!(
            symbol,
            Some(Symbol::FieldName {
                name: "user".to_string()
            })
        );

        // Position at "name"
        let symbol = find_symbol_at_offset(&tree, 15);
        assert_eq!(
            symbol,
            Some(Symbol::FieldName {
                name: "name".to_string()
            })
        );
    }

    #[test]
    fn test_find_fragment_spread() {
        let source = "query { ...UserFields }";
        let parser = Parser::new(source);
        let tree = parser.parse();

        // Position at "UserFields"
        let symbol = find_symbol_at_offset(&tree, 11);
        assert_eq!(
            symbol,
            Some(Symbol::FragmentSpread {
                name: "UserFields".to_string()
            })
        );
    }

    #[test]
    fn test_find_type_name_in_inline_fragment() {
        let source = "query { ... on User { name } }";
        let parser = Parser::new(source);
        let tree = parser.parse();

        // Position at "User"
        let symbol = find_symbol_at_offset(&tree, 15);
        assert_eq!(
            symbol,
            Some(Symbol::TypeName {
                name: "User".to_string()
            })
        );
    }
}
