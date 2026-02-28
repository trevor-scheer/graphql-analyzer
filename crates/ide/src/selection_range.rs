//! Selection range feature implementation.
//!
//! This module provides smart expand/shrink selection functionality.
//! It returns nested selection ranges from innermost to outermost:
//! - field name → field with args → selection set → operation body → operation → document

use apollo_parser::cst::{self, CstNode};

use crate::helpers::{find_block_for_position, offset_range_to_range, position_to_offset};
use crate::types::{FilePath, Position, Range, SelectionRange};
use crate::FileRegistry;

/// Get selection ranges at multiple positions in a file.
///
/// Returns a `SelectionRange` for each input position, forming a linked list
/// from the innermost syntax element to the outermost (document).
pub fn selection_ranges(
    db: &dyn graphql_syntax::GraphQLSyntaxDatabase,
    registry: &FileRegistry,
    file: &FilePath,
    positions: &[Position],
) -> Vec<Option<SelectionRange>> {
    let (content, metadata) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return positions.iter().map(|_| None).collect();
        };
        let Some(content) = registry.get_content(file_id) else {
            return positions.iter().map(|_| None).collect();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return positions.iter().map(|_| None).collect();
        };
        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);

    positions
        .iter()
        .map(|position| selection_range_at_position(&parse, *position))
        .collect()
}

/// Get selection range at a single position
fn selection_range_at_position(
    parse: &graphql_syntax::Parse,
    position: Position,
) -> Option<SelectionRange> {
    let (block_context, adjusted_position) = find_block_for_position(parse, position)?;

    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;

    // Find all ancestor ranges at this offset (outermost to innermost)
    let ranges = find_ancestor_ranges(
        block_context.tree,
        &block_line_index,
        offset,
        block_context.line_offset,
    );

    SelectionRange::from_ranges(&ranges)
}

/// Find all ancestor ranges at the given offset.
///
/// Returns ranges from outermost (document) to innermost (token at cursor).
fn find_ancestor_ranges(
    tree: &apollo_parser::SyntaxTree,
    line_index: &graphql_syntax::LineIndex,
    byte_offset: usize,
    line_offset: u32,
) -> Vec<Range> {
    let doc = tree.document();

    // Start with the document range
    let doc_range = syntax_range_to_ide_range(doc.syntax(), line_index, line_offset);
    let mut ranges = vec![doc_range];

    // Find the definition containing the offset
    for definition in doc.definitions() {
        let def_syntax = definition.syntax();
        if !contains_offset(def_syntax, byte_offset) {
            continue;
        }

        // Add the definition range
        ranges.push(syntax_range_to_ide_range(
            def_syntax,
            line_index,
            line_offset,
        ));

        // Drill into the specific definition type
        match definition {
            cst::Definition::OperationDefinition(op) => {
                collect_operation_ranges(&op, byte_offset, line_index, line_offset, &mut ranges);
            }
            cst::Definition::FragmentDefinition(frag) => {
                collect_fragment_ranges(&frag, byte_offset, line_index, line_offset, &mut ranges);
            }
            cst::Definition::SchemaDefinition(schema) => {
                // Schema definition - add root operation types if cursor is there
                for root_op in schema.root_operation_type_definitions() {
                    if contains_offset(root_op.syntax(), byte_offset) {
                        ranges.push(syntax_range_to_ide_range(
                            root_op.syntax(),
                            line_index,
                            line_offset,
                        ));
                        // Add the type name if cursor is on it
                        if let Some(named_type) = root_op.named_type() {
                            if contains_offset(named_type.syntax(), byte_offset) {
                                ranges.push(syntax_range_to_ide_range(
                                    named_type.syntax(),
                                    line_index,
                                    line_offset,
                                ));
                            }
                        }
                    }
                }
            }
            cst::Definition::ObjectTypeDefinition(obj) => {
                collect_object_type_ranges(&obj, byte_offset, line_index, line_offset, &mut ranges);
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                collect_interface_type_ranges(
                    &iface,
                    byte_offset,
                    line_index,
                    line_offset,
                    &mut ranges,
                );
            }
            cst::Definition::UnionTypeDefinition(union_def) => {
                collect_union_type_ranges(
                    &union_def,
                    byte_offset,
                    line_index,
                    line_offset,
                    &mut ranges,
                );
            }
            cst::Definition::EnumTypeDefinition(enum_def) => {
                collect_enum_type_ranges(
                    &enum_def,
                    byte_offset,
                    line_index,
                    line_offset,
                    &mut ranges,
                );
            }
            cst::Definition::ScalarTypeDefinition(scalar) => {
                if let Some(name) = scalar.name() {
                    if contains_offset(name.syntax(), byte_offset) {
                        ranges.push(syntax_range_to_ide_range(
                            name.syntax(),
                            line_index,
                            line_offset,
                        ));
                    }
                }
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                collect_input_type_ranges(
                    &input,
                    byte_offset,
                    line_index,
                    line_offset,
                    &mut ranges,
                );
            }
            cst::Definition::DirectiveDefinition(dir_def) => {
                collect_directive_definition_ranges(
                    &dir_def,
                    byte_offset,
                    line_index,
                    line_offset,
                    &mut ranges,
                );
            }
            // Handle type extensions (similar structure to definitions)
            cst::Definition::ObjectTypeExtension(_)
            | cst::Definition::InterfaceTypeExtension(_)
            | cst::Definition::UnionTypeExtension(_)
            | cst::Definition::EnumTypeExtension(_)
            | cst::Definition::ScalarTypeExtension(_)
            | cst::Definition::InputObjectTypeExtension(_)
            | cst::Definition::SchemaExtension(_) => {
                // Extensions follow similar patterns - keep it simple for now
            }
        }

        break; // Only process the first matching definition
    }

    ranges
}

/// Collect ranges for an operation definition
fn collect_operation_ranges(
    op: &cst::OperationDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    // Check if cursor is in operation name
    if let Some(name) = op.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    // Check variable definitions
    if let Some(var_defs) = op.variable_definitions() {
        if contains_offset(var_defs.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                var_defs.syntax(),
                line_index,
                line_offset,
            ));
            for var_def in var_defs.variable_definitions() {
                if contains_offset(var_def.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        var_def.syntax(),
                        line_index,
                        line_offset,
                    ));
                    // Add variable name if cursor is on it
                    if let Some(var) = var_def.variable() {
                        if contains_offset(var.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                var.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                    // Add type if cursor is on it
                    if let Some(ty) = var_def.ty() {
                        collect_type_ranges(&ty, byte_offset, line_index, line_offset, ranges);
                    }
                    return;
                }
            }
            return;
        }
    }

    // Check directives
    if let Some(directives) = op.directives() {
        if contains_offset(directives.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                directives.syntax(),
                line_index,
                line_offset,
            ));
            collect_directives_ranges(&directives, byte_offset, line_index, line_offset, ranges);
            return;
        }
    }

    // Check selection set
    if let Some(selection_set) = op.selection_set() {
        collect_selection_set_ranges(&selection_set, byte_offset, line_index, line_offset, ranges);
    }
}

/// Collect ranges for a fragment definition
fn collect_fragment_ranges(
    frag: &cst::FragmentDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    // Check fragment name
    if let Some(name) = frag.fragment_name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    // Check type condition
    if let Some(type_cond) = frag.type_condition() {
        if contains_offset(type_cond.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                type_cond.syntax(),
                line_index,
                line_offset,
            ));
            if let Some(named_type) = type_cond.named_type() {
                if contains_offset(named_type.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        named_type.syntax(),
                        line_index,
                        line_offset,
                    ));
                }
            }
            return;
        }
    }

    // Check directives
    if let Some(directives) = frag.directives() {
        if contains_offset(directives.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                directives.syntax(),
                line_index,
                line_offset,
            ));
            collect_directives_ranges(&directives, byte_offset, line_index, line_offset, ranges);
            return;
        }
    }

    // Check selection set
    if let Some(selection_set) = frag.selection_set() {
        collect_selection_set_ranges(&selection_set, byte_offset, line_index, line_offset, ranges);
    }
}

/// Collect ranges within a selection set (recursive for nested fields)
fn collect_selection_set_ranges(
    selection_set: &cst::SelectionSet,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if !contains_offset(selection_set.syntax(), byte_offset) {
        return;
    }

    // Add the selection set range
    ranges.push(syntax_range_to_ide_range(
        selection_set.syntax(),
        line_index,
        line_offset,
    ));

    // Find the selection containing the cursor
    for selection in selection_set.selections() {
        let selection_syntax = selection.syntax();
        if !contains_offset(selection_syntax, byte_offset) {
            continue;
        }

        match selection {
            cst::Selection::Field(field) => {
                collect_field_ranges(&field, byte_offset, line_index, line_offset, ranges);
            }
            cst::Selection::FragmentSpread(spread) => {
                // Add the spread range
                ranges.push(syntax_range_to_ide_range(
                    spread.syntax(),
                    line_index,
                    line_offset,
                ));

                // Add fragment name if cursor is on it
                if let Some(name) = spread.fragment_name() {
                    if contains_offset(name.syntax(), byte_offset) {
                        ranges.push(syntax_range_to_ide_range(
                            name.syntax(),
                            line_index,
                            line_offset,
                        ));
                    }
                }

                // Check directives
                if let Some(directives) = spread.directives() {
                    if contains_offset(directives.syntax(), byte_offset) {
                        ranges.push(syntax_range_to_ide_range(
                            directives.syntax(),
                            line_index,
                            line_offset,
                        ));
                        collect_directives_ranges(
                            &directives,
                            byte_offset,
                            line_index,
                            line_offset,
                            ranges,
                        );
                    }
                }
            }
            cst::Selection::InlineFragment(inline_frag) => {
                // Add the inline fragment range
                ranges.push(syntax_range_to_ide_range(
                    inline_frag.syntax(),
                    line_index,
                    line_offset,
                ));

                // Check type condition
                if let Some(type_cond) = inline_frag.type_condition() {
                    if contains_offset(type_cond.syntax(), byte_offset) {
                        ranges.push(syntax_range_to_ide_range(
                            type_cond.syntax(),
                            line_index,
                            line_offset,
                        ));
                        if let Some(named_type) = type_cond.named_type() {
                            if contains_offset(named_type.syntax(), byte_offset) {
                                ranges.push(syntax_range_to_ide_range(
                                    named_type.syntax(),
                                    line_index,
                                    line_offset,
                                ));
                            }
                        }
                        return;
                    }
                }

                // Check directives
                if let Some(directives) = inline_frag.directives() {
                    if contains_offset(directives.syntax(), byte_offset) {
                        ranges.push(syntax_range_to_ide_range(
                            directives.syntax(),
                            line_index,
                            line_offset,
                        ));
                        collect_directives_ranges(
                            &directives,
                            byte_offset,
                            line_index,
                            line_offset,
                            ranges,
                        );
                        return;
                    }
                }

                // Check nested selection set
                if let Some(nested_selection_set) = inline_frag.selection_set() {
                    collect_selection_set_ranges(
                        &nested_selection_set,
                        byte_offset,
                        line_index,
                        line_offset,
                        ranges,
                    );
                }
            }
        }

        break; // Only process the first matching selection
    }
}

/// Collect ranges for a field
fn collect_field_ranges(
    field: &cst::Field,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    // Add the field range (entire field including nested selection set)
    ranges.push(syntax_range_to_ide_range(
        field.syntax(),
        line_index,
        line_offset,
    ));

    // Check alias
    if let Some(alias) = field.alias() {
        if contains_offset(alias.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                alias.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    // Check field name
    if let Some(name) = field.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    // Check arguments
    if let Some(arguments) = field.arguments() {
        if contains_offset(arguments.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                arguments.syntax(),
                line_index,
                line_offset,
            ));
            for arg in arguments.arguments() {
                if contains_offset(arg.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        arg.syntax(),
                        line_index,
                        line_offset,
                    ));
                    // Add argument name if cursor is on it
                    if let Some(name) = arg.name() {
                        if contains_offset(name.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                name.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                    // Add argument value if cursor is on it
                    if let Some(value) = arg.value() {
                        collect_value_ranges(&value, byte_offset, line_index, line_offset, ranges);
                    }
                    return;
                }
            }
            return;
        }
    }

    // Check directives
    if let Some(directives) = field.directives() {
        if contains_offset(directives.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                directives.syntax(),
                line_index,
                line_offset,
            ));
            collect_directives_ranges(&directives, byte_offset, line_index, line_offset, ranges);
            return;
        }
    }

    // Check nested selection set (recurse)
    if let Some(selection_set) = field.selection_set() {
        collect_selection_set_ranges(&selection_set, byte_offset, line_index, line_offset, ranges);
    }
}

/// Collect ranges for directives
fn collect_directives_ranges(
    directives: &cst::Directives,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    for directive in directives.directives() {
        if contains_offset(directive.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                directive.syntax(),
                line_index,
                line_offset,
            ));

            // Add directive name if cursor is on it
            if let Some(name) = directive.name() {
                if contains_offset(name.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        name.syntax(),
                        line_index,
                        line_offset,
                    ));
                    return;
                }
            }

            // Check arguments
            if let Some(arguments) = directive.arguments() {
                if contains_offset(arguments.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        arguments.syntax(),
                        line_index,
                        line_offset,
                    ));
                    for arg in arguments.arguments() {
                        if contains_offset(arg.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                arg.syntax(),
                                line_index,
                                line_offset,
                            ));
                            if let Some(name) = arg.name() {
                                if contains_offset(name.syntax(), byte_offset) {
                                    ranges.push(syntax_range_to_ide_range(
                                        name.syntax(),
                                        line_index,
                                        line_offset,
                                    ));
                                }
                            }
                            if let Some(value) = arg.value() {
                                collect_value_ranges(
                                    &value,
                                    byte_offset,
                                    line_index,
                                    line_offset,
                                    ranges,
                                );
                            }
                        }
                    }
                }
            }

            return;
        }
    }
}

/// Collect ranges for a value (handles nested objects, lists, etc.)
fn collect_value_ranges(
    value: &cst::Value,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if !contains_offset(value.syntax(), byte_offset) {
        return;
    }

    ranges.push(syntax_range_to_ide_range(
        value.syntax(),
        line_index,
        line_offset,
    ));

    match value {
        cst::Value::ListValue(list) => {
            for item in list.values() {
                if contains_offset(item.syntax(), byte_offset) {
                    collect_value_ranges(&item, byte_offset, line_index, line_offset, ranges);
                    break;
                }
            }
        }
        cst::Value::ObjectValue(obj) => {
            for field in obj.object_fields() {
                if contains_offset(field.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        field.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(name) = field.name() {
                        if contains_offset(name.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                name.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                    if let Some(inner_value) = field.value() {
                        collect_value_ranges(
                            &inner_value,
                            byte_offset,
                            line_index,
                            line_offset,
                            ranges,
                        );
                    }
                    break;
                }
            }
        }
        _ => {}
    }
}

/// Collect ranges for a type reference
fn collect_type_ranges(
    ty: &cst::Type,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if !contains_offset(ty.syntax(), byte_offset) {
        return;
    }

    ranges.push(syntax_range_to_ide_range(
        ty.syntax(),
        line_index,
        line_offset,
    ));

    match ty {
        cst::Type::NamedType(named) => {
            if let Some(name) = named.name() {
                if contains_offset(name.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        name.syntax(),
                        line_index,
                        line_offset,
                    ));
                }
            }
        }
        cst::Type::ListType(list) => {
            if let Some(inner_ty) = list.ty() {
                collect_type_ranges(&inner_ty, byte_offset, line_index, line_offset, ranges);
            }
        }
        cst::Type::NonNullType(non_null) => {
            if let Some(named) = non_null.named_type() {
                if contains_offset(named.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        named.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(name) = named.name() {
                        if contains_offset(name.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                name.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                }
            }
            if let Some(list) = non_null.list_type() {
                if contains_offset(list.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        list.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(inner_ty) = list.ty() {
                        collect_type_ranges(
                            &inner_ty,
                            byte_offset,
                            line_index,
                            line_offset,
                            ranges,
                        );
                    }
                }
            }
        }
    }
}

/// Collect ranges for directive definitions
fn collect_directive_definition_ranges(
    dir_def: &cst::DirectiveDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if let Some(name) = dir_def.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(args) = dir_def.arguments_definition() {
        if contains_offset(args.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                args.syntax(),
                line_index,
                line_offset,
            ));
            for input_value in args.input_value_definitions() {
                if contains_offset(input_value.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        input_value.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(name) = input_value.name() {
                        if contains_offset(name.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                name.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                    if let Some(ty) = input_value.ty() {
                        collect_type_ranges(&ty, byte_offset, line_index, line_offset, ranges);
                    }
                }
            }
        }
    }
}

/// Collect ranges for object type definitions
fn collect_object_type_ranges(
    obj: &cst::ObjectTypeDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if let Some(name) = obj.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(implements) = obj.implements_interfaces() {
        if contains_offset(implements.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                implements.syntax(),
                line_index,
                line_offset,
            ));
            for named_type in implements.named_types() {
                if contains_offset(named_type.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        named_type.syntax(),
                        line_index,
                        line_offset,
                    ));
                }
            }
            return;
        }
    }

    if let Some(directives) = obj.directives() {
        if contains_offset(directives.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                directives.syntax(),
                line_index,
                line_offset,
            ));
            collect_directives_ranges(&directives, byte_offset, line_index, line_offset, ranges);
            return;
        }
    }

    if let Some(fields_def) = obj.fields_definition() {
        if contains_offset(fields_def.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                fields_def.syntax(),
                line_index,
                line_offset,
            ));
            for field in fields_def.field_definitions() {
                if contains_offset(field.syntax(), byte_offset) {
                    collect_schema_field_ranges(
                        &field,
                        byte_offset,
                        line_index,
                        line_offset,
                        ranges,
                    );
                }
            }
        }
    }
}

/// Collect ranges for interface type definitions
fn collect_interface_type_ranges(
    iface: &cst::InterfaceTypeDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if let Some(name) = iface.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(directives) = iface.directives() {
        if contains_offset(directives.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                directives.syntax(),
                line_index,
                line_offset,
            ));
            collect_directives_ranges(&directives, byte_offset, line_index, line_offset, ranges);
            return;
        }
    }

    if let Some(fields_def) = iface.fields_definition() {
        if contains_offset(fields_def.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                fields_def.syntax(),
                line_index,
                line_offset,
            ));
            for field in fields_def.field_definitions() {
                if contains_offset(field.syntax(), byte_offset) {
                    collect_schema_field_ranges(
                        &field,
                        byte_offset,
                        line_index,
                        line_offset,
                        ranges,
                    );
                }
            }
        }
    }
}

/// Collect ranges for union type definitions
fn collect_union_type_ranges(
    union_def: &cst::UnionTypeDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if let Some(name) = union_def.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(members) = union_def.union_member_types() {
        if contains_offset(members.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                members.syntax(),
                line_index,
                line_offset,
            ));
            for member in members.named_types() {
                if contains_offset(member.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        member.syntax(),
                        line_index,
                        line_offset,
                    ));
                }
            }
        }
    }
}

/// Collect ranges for enum type definitions
fn collect_enum_type_ranges(
    enum_def: &cst::EnumTypeDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if let Some(name) = enum_def.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(values_def) = enum_def.enum_values_definition() {
        if contains_offset(values_def.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                values_def.syntax(),
                line_index,
                line_offset,
            ));
            for value in values_def.enum_value_definitions() {
                if contains_offset(value.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        value.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(enum_val) = value.enum_value() {
                        if contains_offset(enum_val.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                enum_val.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// Collect ranges for input object type definitions
fn collect_input_type_ranges(
    input: &cst::InputObjectTypeDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    if let Some(name) = input.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(fields_def) = input.input_fields_definition() {
        if contains_offset(fields_def.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                fields_def.syntax(),
                line_index,
                line_offset,
            ));
            for input_value in fields_def.input_value_definitions() {
                if contains_offset(input_value.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        input_value.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(name) = input_value.name() {
                        if contains_offset(name.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                name.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                    if let Some(ty) = input_value.ty() {
                        collect_type_ranges(&ty, byte_offset, line_index, line_offset, ranges);
                    }
                }
            }
        }
    }
}

/// Collect ranges for schema field definitions
fn collect_schema_field_ranges(
    field: &cst::FieldDefinition,
    byte_offset: usize,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    ranges: &mut Vec<Range>,
) {
    ranges.push(syntax_range_to_ide_range(
        field.syntax(),
        line_index,
        line_offset,
    ));

    if let Some(name) = field.name() {
        if contains_offset(name.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                name.syntax(),
                line_index,
                line_offset,
            ));
            return;
        }
    }

    if let Some(args) = field.arguments_definition() {
        if contains_offset(args.syntax(), byte_offset) {
            ranges.push(syntax_range_to_ide_range(
                args.syntax(),
                line_index,
                line_offset,
            ));
            for input_value in args.input_value_definitions() {
                if contains_offset(input_value.syntax(), byte_offset) {
                    ranges.push(syntax_range_to_ide_range(
                        input_value.syntax(),
                        line_index,
                        line_offset,
                    ));
                    if let Some(name) = input_value.name() {
                        if contains_offset(name.syntax(), byte_offset) {
                            ranges.push(syntax_range_to_ide_range(
                                name.syntax(),
                                line_index,
                                line_offset,
                            ));
                        }
                    }
                    if let Some(ty) = input_value.ty() {
                        collect_type_ranges(&ty, byte_offset, line_index, line_offset, ranges);
                    }
                }
            }
            return;
        }
    }

    if let Some(ty) = field.ty() {
        collect_type_ranges(&ty, byte_offset, line_index, line_offset, ranges);
    }
}

// Helper functions

/// Check if a syntax node contains the given byte offset
fn contains_offset(node: &apollo_parser::SyntaxNode, offset: usize) -> bool {
    let range = node.text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    offset >= start && offset <= end
}

/// Convert a syntax node's range to an IDE range
fn syntax_range_to_ide_range(
    node: &apollo_parser::SyntaxNode,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
) -> Range {
    let range = node.text_range();
    let start_offset: usize = range.start().into();
    let end_offset: usize = range.end().into();

    let base_range = offset_range_to_range(line_index, start_offset, end_offset);

    // Adjust for embedded GraphQL line offset
    if line_offset > 0 {
        Range::new(
            crate::types::Position::new(
                base_range.start.line + line_offset,
                base_range.start.character,
            ),
            crate::types::Position::new(
                base_range.end.line + line_offset,
                base_range.end.character,
            ),
        )
    } else {
        base_range
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnalysisHost;
    use graphql_base_db::{DocumentKind, Language};

    fn test_selection_ranges(
        source: &str,
        cursor_line: u32,
        cursor_col: u32,
    ) -> Option<SelectionRange> {
        let mut host = AnalysisHost::new();
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, source, Language::GraphQL, DocumentKind::Executable);

        let analysis = host.snapshot();
        let position = Position::new(cursor_line, cursor_col);
        let results = analysis.selection_ranges(&path, &[position]);

        results.into_iter().next().flatten()
    }

    fn range_chain_to_strings(selection_range: &SelectionRange, source: &str) -> Vec<String> {
        let lines: Vec<&str> = source.lines().collect();
        let mut result = Vec::new();
        let mut current = Some(selection_range);

        while let Some(sr) = current {
            let start_line = sr.range.start.line as usize;
            let end_line = sr.range.end.line as usize;
            let start_col = sr.range.start.character as usize;
            let end_col = sr.range.end.character as usize;

            let text = if start_line == end_line {
                let line = lines.get(start_line).unwrap_or(&"");
                let end_c = end_col.min(line.len());
                let start_c = start_col.min(end_c);
                line[start_c..end_c].to_string()
            } else {
                // Multi-line: extract first line from start_col, middle lines fully, last line to end_col
                let mut s = String::new();
                if let Some(first_line) = lines.get(start_line) {
                    s.push_str(&first_line[start_col.min(first_line.len())..]);
                }
                for mid in (start_line + 1)..end_line {
                    if let Some(line) = lines.get(mid) {
                        s.push('\n');
                        s.push_str(line);
                    }
                }
                if let Some(last_line) = lines.get(end_line) {
                    s.push('\n');
                    s.push_str(&last_line[..end_col.min(last_line.len())]);
                }
                s
            };

            result.push(text);
            current = sr.parent.as_deref();
        }

        result
    }

    #[test]
    fn test_selection_range_on_field_name() {
        let source = "query GetUser {\n  user {\n    name\n  }\n}";
        //             0         1         2         3
        //             0123456789012345678901234567890123456789
        // Line 0: "query GetUser {"
        // Line 1: "  user {"
        // Line 2: "    name"
        // Line 3: "  }"
        // Line 4: "}"

        // Cursor on "name" (line 2, col 4-8)
        let result = test_selection_ranges(source, 2, 5);
        assert!(result.is_some(), "Expected selection range for field name");

        let sr = result.unwrap();
        let chain = range_chain_to_strings(&sr, source);

        // Innermost should be "name", then the full field, then selection set, then user field, etc.
        assert!(
            chain.iter().any(|s| s.trim() == "name"),
            "Should have 'name' in chain: {chain:?}"
        );
    }

    #[test]
    fn test_selection_range_on_operation_name() {
        let source = "query GetUser {\n  id\n}";

        // Cursor on "GetUser" (line 0, col 6-13)
        let result = test_selection_ranges(source, 0, 8);
        assert!(
            result.is_some(),
            "Expected selection range for operation name"
        );

        let sr = result.unwrap();
        let chain = range_chain_to_strings(&sr, source);

        // Should have "GetUser" in chain
        assert!(
            chain.iter().any(|s| s.trim() == "GetUser"),
            "Should have 'GetUser' in chain: {chain:?}"
        );
    }

    #[test]
    fn test_selection_range_on_fragment_spread() {
        let source =
            "query {\n  user {\n    ...UserFields\n  }\n}\nfragment UserFields on User {\n  id\n}";

        // Cursor on "UserFields" in the spread (line 2, col 7-17)
        let result = test_selection_ranges(source, 2, 10);
        assert!(
            result.is_some(),
            "Expected selection range for fragment spread"
        );

        let sr = result.unwrap();
        let chain = range_chain_to_strings(&sr, source);

        // Should have "UserFields" in chain
        assert!(
            chain.iter().any(|s| s.contains("UserFields")),
            "Should have 'UserFields' in chain: {chain:?}"
        );
    }

    #[test]
    fn test_selection_range_hierarchy() {
        let source = "query {\n  user {\n    id\n  }\n}";

        // Cursor on "id" (line 2, col 4-6)
        let result = test_selection_ranges(source, 2, 5);
        assert!(result.is_some(), "Expected selection range");

        let sr = result.unwrap();

        // Count the chain depth - should have multiple levels
        let mut depth = 0;
        let mut current: Option<&SelectionRange> = Some(&sr);
        while let Some(s) = current {
            depth += 1;
            current = s.parent.as_deref();
        }

        // Expected hierarchy: id -> { id } -> user { ... } -> { user { ... } } -> query { ... } -> document
        // At minimum we should have 3+ levels
        assert!(
            depth >= 3,
            "Expected at least 3 levels of selection, got {depth}"
        );
    }
}
