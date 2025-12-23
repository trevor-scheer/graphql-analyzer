//! Symbol identification at positions
//!
//! This module provides utilities for finding GraphQL symbols at specific positions
//! in source code, using apollo-parser's CST for position lookups.

use apollo_parser::cst::{self, CstNode};

/// A GraphQL symbol identified at a specific position
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Some variants are for future implementation
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
}

/// Find the symbol at a specific byte offset in the document
pub fn find_symbol_at_offset(
    tree: &apollo_parser::SyntaxTree,
    byte_offset: usize,
) -> Option<Symbol> {
    let doc = tree.document();

    // Search through all definitions
    for definition in doc.definitions() {
        if let Some(symbol) = check_definition(&definition, byte_offset) {
            return Some(symbol);
        }
    }

    None
}

fn check_definition(definition: &cst::Definition, byte_offset: usize) -> Option<Symbol> {
    match definition {
        cst::Definition::OperationDefinition(op) => check_operation(op, byte_offset),
        cst::Definition::FragmentDefinition(frag) => check_fragment_definition(frag, byte_offset),
        _ => None,
    }
}

fn check_operation(op: &cst::OperationDefinition, byte_offset: usize) -> Option<Symbol> {
    // Check operation name
    if let Some(name) = op.name() {
        if is_within_range(&name, byte_offset) {
            return Some(Symbol::OperationName {
                name: name.text().to_string(),
            });
        }
    }

    // Check selection set
    if let Some(selection_set) = op.selection_set() {
        if let Some(symbol) = check_selection_set(&selection_set, byte_offset) {
            return Some(symbol);
        }
    }

    None
}

fn check_fragment_definition(frag: &cst::FragmentDefinition, byte_offset: usize) -> Option<Symbol> {
    // Check type condition
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

    // Check selection set
    if let Some(selection_set) = frag.selection_set() {
        if let Some(symbol) = check_selection_set(&selection_set, byte_offset) {
            return Some(symbol);
        }
    }

    None
}

fn check_selection_set(selection_set: &cst::SelectionSet, byte_offset: usize) -> Option<Symbol> {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                // Check field name
                if let Some(name) = field.name() {
                    if is_within_range(&name, byte_offset) {
                        return Some(Symbol::FieldName {
                            name: name.text().to_string(),
                        });
                    }
                }

                // Check arguments
                if let Some(arguments) = field.arguments() {
                    for arg in arguments.arguments() {
                        if let Some(name) = arg.name() {
                            if is_within_range(&name, byte_offset) {
                                return Some(Symbol::ArgumentName {
                                    name: name.text().to_string(),
                                });
                            }
                        }
                    }
                }

                // Check nested selection set
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
                // Check type condition
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

                // Check nested selection set
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

/// Find all fragment spreads with a specific name in a syntax tree
pub fn find_fragment_spreads(
    tree: &apollo_parser::SyntaxTree,
    fragment_name: &str,
) -> Option<Vec<usize>> {
    let mut offsets = Vec::new();
    let doc = tree.document();

    // Search through all definitions
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

    // Search through all definitions
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
    // Check implements interfaces
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

    // Check field types
    for field in obj
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        // Check argument types
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
    // Check implements interfaces
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

    // Check field types
    for field in iface
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        // Check argument types
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
    // Check union members
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
    // Check input field types
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
    // Check implements interfaces
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

    // Check field types
    for field in ext
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        // Check argument types
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
    // Check implements interfaces
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

    // Check field types
    for field in ext
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
    {
        if let Some(ty) = field.ty() {
            find_type_refs_in_type(&ty, type_name, offsets);
        }

        // Check argument types
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
    // Check union members
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
    // Check input field types
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
