//! Visitor pattern for GraphQL CST traversal.
//!
//! This module provides a visitor trait that allows traversing the GraphQL CST
//! with custom logic at each node type. Default implementations do nothing,
//! so you only need to override the methods you care about.
//!
//! # Example
//!
//! ```
//! use graphql_apollo_ext::{CstVisitor, walk_document};
//! use apollo_parser::cst;
//!
//! struct FieldCounter(usize);
//!
//! impl CstVisitor for FieldCounter {
//!     fn visit_field(&mut self, _field: &cst::Field) {
//!         self.0 += 1;
//!     }
//! }
//!
//! let source = "query { user { name email } }";
//! let tree = apollo_parser::Parser::new(source).parse();
//! let mut counter = FieldCounter(0);
//! walk_document(&mut counter, &tree);
//! assert_eq!(counter.0, 3); // user, name, email
//! ```

use apollo_parser::cst;

/// A visitor for traversing GraphQL CST nodes.
///
/// All methods have default empty implementations. Override only the methods
/// you need. The `walk_*` functions handle traversal; visitor methods are
/// called at each node.
///
/// Methods prefixed with `enter_` are called before visiting children,
/// and `exit_` methods are called after. Simple `visit_` methods are called
/// once without separate enter/exit phases.
#[allow(unused_variables)]
pub trait CstVisitor {
    // =========================================================================
    // Document-level visitors
    // =========================================================================

    /// Called when entering a document (before visiting definitions)
    fn enter_document(&mut self, doc: &cst::Document) {}

    /// Called when exiting a document (after visiting all definitions)
    fn exit_document(&mut self, doc: &cst::Document) {}

    // =========================================================================
    // Definition visitors
    // =========================================================================

    /// Called for each definition in the document
    fn visit_definition(&mut self, def: &cst::Definition) {}

    /// Called when entering an operation definition
    fn enter_operation(&mut self, op: &cst::OperationDefinition) {}

    /// Called when exiting an operation definition
    fn exit_operation(&mut self, op: &cst::OperationDefinition) {}

    /// Called when entering a fragment definition
    fn enter_fragment_definition(&mut self, frag: &cst::FragmentDefinition) {}

    /// Called when exiting a fragment definition
    fn exit_fragment_definition(&mut self, frag: &cst::FragmentDefinition) {}

    // =========================================================================
    // Schema definition visitors
    // =========================================================================

    /// Called for object type definitions
    fn visit_object_type(&mut self, obj: &cst::ObjectTypeDefinition) {}

    /// Called for interface type definitions
    fn visit_interface_type(&mut self, iface: &cst::InterfaceTypeDefinition) {}

    /// Called for union type definitions
    fn visit_union_type(&mut self, union: &cst::UnionTypeDefinition) {}

    /// Called for enum type definitions
    fn visit_enum_type(&mut self, enum_def: &cst::EnumTypeDefinition) {}

    /// Called for scalar type definitions
    fn visit_scalar_type(&mut self, scalar: &cst::ScalarTypeDefinition) {}

    /// Called for input object type definitions
    fn visit_input_object_type(&mut self, input: &cst::InputObjectTypeDefinition) {}

    /// Called for directive definitions
    fn visit_directive_definition(&mut self, dir: &cst::DirectiveDefinition) {}

    /// Called for schema definitions
    fn visit_schema_definition(&mut self, schema: &cst::SchemaDefinition) {}

    /// Called for field definitions in object/interface types
    fn visit_field_definition(&mut self, field: &cst::FieldDefinition) {}

    /// Called for input value definitions (arguments, input fields)
    fn visit_input_value_definition(&mut self, input: &cst::InputValueDefinition) {}

    /// Called for enum value definitions
    fn visit_enum_value_definition(&mut self, value: &cst::EnumValueDefinition) {}

    // =========================================================================
    // Selection set visitors
    // =========================================================================

    /// Called when entering a selection set
    fn enter_selection_set(&mut self, set: &cst::SelectionSet) {}

    /// Called when exiting a selection set
    fn exit_selection_set(&mut self, set: &cst::SelectionSet) {}

    /// Called for each field in a selection set
    fn visit_field(&mut self, field: &cst::Field) {}

    /// Called for each fragment spread (`...FragmentName`)
    fn visit_fragment_spread(&mut self, spread: &cst::FragmentSpread) {}

    /// Called when entering an inline fragment (... on Type { })
    fn enter_inline_fragment(&mut self, inline: &cst::InlineFragment) {}

    /// Called when exiting an inline fragment
    fn exit_inline_fragment(&mut self, inline: &cst::InlineFragment) {}

    // =========================================================================
    // Value visitors
    // =========================================================================

    /// Called for variable references ($varName)
    fn visit_variable(&mut self, var: &cst::Variable) {}

    /// Called for each argument in field/directive arguments
    fn visit_argument(&mut self, arg: &cst::Argument) {}

    /// Called for each directive (@directive)
    fn visit_directive(&mut self, dir: &cst::Directive) {}

    /// Called when entering a variable definition
    fn enter_variable_definition(&mut self, var_def: &cst::VariableDefinition) {}

    /// Called for variable definitions in operations
    fn visit_variable_definition(&mut self, var_def: &cst::VariableDefinition) {}

    /// Called when exiting a variable definition
    fn exit_variable_definition(&mut self, var_def: &cst::VariableDefinition) {}

    /// Called for any value node (can override for specific value handling)
    fn visit_value(&mut self, value: &cst::Value) {}

    // =========================================================================
    // Type reference visitors
    // =========================================================================

    /// Called for type references (String, [String!]!, etc.)
    fn visit_type(&mut self, ty: &cst::Type) {}

    /// Called for named types (the base type name)
    fn visit_named_type(&mut self, named: &cst::NamedType) {}
}

// =============================================================================
// Walk functions - these drive the traversal
// =============================================================================

/// Walk a parsed document with the given visitor.
///
/// This is the main entry point for traversing a GraphQL document.
pub fn walk_document<V: CstVisitor>(visitor: &mut V, tree: &apollo_parser::SyntaxTree) {
    let doc = tree.document();
    visitor.enter_document(&doc);

    for definition in doc.definitions() {
        walk_definition(visitor, &definition);
    }

    visitor.exit_document(&doc);
}

/// Walk a single definition.
pub fn walk_definition<V: CstVisitor>(visitor: &mut V, def: &cst::Definition) {
    visitor.visit_definition(def);

    match def {
        cst::Definition::OperationDefinition(op) => walk_operation(visitor, op),
        cst::Definition::FragmentDefinition(frag) => walk_fragment_definition(visitor, frag),
        cst::Definition::ObjectTypeDefinition(obj) => walk_object_type(visitor, obj),
        cst::Definition::InterfaceTypeDefinition(iface) => walk_interface_type(visitor, iface),
        cst::Definition::UnionTypeDefinition(union) => walk_union_type(visitor, union),
        cst::Definition::EnumTypeDefinition(enum_def) => walk_enum_type(visitor, enum_def),
        cst::Definition::ScalarTypeDefinition(scalar) => walk_scalar_type(visitor, scalar),
        cst::Definition::InputObjectTypeDefinition(input) => walk_input_object_type(visitor, input),
        cst::Definition::DirectiveDefinition(dir) => walk_directive_definition(visitor, dir),
        cst::Definition::SchemaDefinition(schema) => walk_schema_definition(visitor, schema),
        // Extensions
        cst::Definition::ObjectTypeExtension(ext) => walk_object_type_extension(visitor, ext),
        cst::Definition::InterfaceTypeExtension(ext) => walk_interface_type_extension(visitor, ext),
        cst::Definition::UnionTypeExtension(ext) => walk_union_type_extension(visitor, ext),
        cst::Definition::EnumTypeExtension(ext) => walk_enum_type_extension(visitor, ext),
        cst::Definition::ScalarTypeExtension(ext) => walk_scalar_type_extension(visitor, ext),
        cst::Definition::InputObjectTypeExtension(ext) => {
            walk_input_object_type_extension(visitor, ext);
        }
        cst::Definition::SchemaExtension(ext) => walk_schema_extension(visitor, ext),
    }
}

/// Walk an operation definition.
pub fn walk_operation<V: CstVisitor>(visitor: &mut V, op: &cst::OperationDefinition) {
    visitor.enter_operation(op);

    if let Some(var_defs) = op.variable_definitions() {
        for var_def in var_defs.variable_definitions() {
            walk_variable_definition(visitor, &var_def);
        }
    }

    if let Some(directives) = op.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(selection_set) = op.selection_set() {
        walk_selection_set(visitor, &selection_set);
    }

    visitor.exit_operation(op);
}

/// Walk a fragment definition.
pub fn walk_fragment_definition<V: CstVisitor>(visitor: &mut V, frag: &cst::FragmentDefinition) {
    visitor.enter_fragment_definition(frag);

    if let Some(directives) = frag.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(selection_set) = frag.selection_set() {
        walk_selection_set(visitor, &selection_set);
    }

    visitor.exit_fragment_definition(frag);
}

/// Walk a selection set.
pub fn walk_selection_set<V: CstVisitor>(visitor: &mut V, set: &cst::SelectionSet) {
    visitor.enter_selection_set(set);

    for selection in set.selections() {
        match selection {
            cst::Selection::Field(field) => walk_field(visitor, &field),
            cst::Selection::FragmentSpread(spread) => walk_fragment_spread(visitor, &spread),
            cst::Selection::InlineFragment(inline) => walk_inline_fragment(visitor, &inline),
        }
    }

    visitor.exit_selection_set(set);
}

/// Walk a field selection.
pub fn walk_field<V: CstVisitor>(visitor: &mut V, field: &cst::Field) {
    visitor.visit_field(field);

    if let Some(arguments) = field.arguments() {
        walk_arguments(visitor, &arguments);
    }

    if let Some(directives) = field.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(selection_set) = field.selection_set() {
        walk_selection_set(visitor, &selection_set);
    }
}

/// Walk a fragment spread.
pub fn walk_fragment_spread<V: CstVisitor>(visitor: &mut V, spread: &cst::FragmentSpread) {
    visitor.visit_fragment_spread(spread);

    if let Some(directives) = spread.directives() {
        walk_directives(visitor, &directives);
    }
}

/// Walk an inline fragment.
pub fn walk_inline_fragment<V: CstVisitor>(visitor: &mut V, inline: &cst::InlineFragment) {
    visitor.enter_inline_fragment(inline);

    if let Some(directives) = inline.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(selection_set) = inline.selection_set() {
        walk_selection_set(visitor, &selection_set);
    }

    visitor.exit_inline_fragment(inline);
}

/// Walk arguments.
pub fn walk_arguments<V: CstVisitor>(visitor: &mut V, args: &cst::Arguments) {
    for arg in args.arguments() {
        visitor.visit_argument(&arg);
        if let Some(value) = arg.value() {
            walk_value(visitor, &value);
        }
    }
}

/// Walk directives.
pub fn walk_directives<V: CstVisitor>(visitor: &mut V, directives: &cst::Directives) {
    for directive in directives.directives() {
        visitor.visit_directive(&directive);
        if let Some(arguments) = directive.arguments() {
            walk_arguments(visitor, &arguments);
        }
    }
}

/// Walk a variable definition.
pub fn walk_variable_definition<V: CstVisitor>(visitor: &mut V, var_def: &cst::VariableDefinition) {
    visitor.enter_variable_definition(var_def);
    visitor.visit_variable_definition(var_def);

    if let Some(var) = var_def.variable() {
        visitor.visit_variable(&var);
    }

    if let Some(ty) = var_def.ty() {
        walk_type(visitor, &ty);
    }

    if let Some(default) = var_def.default_value() {
        if let Some(value) = default.value() {
            walk_value(visitor, &value);
        }
    }

    if let Some(directives) = var_def.directives() {
        walk_directives(visitor, &directives);
    }

    visitor.exit_variable_definition(var_def);
}

/// Walk a value (recursively handles lists and objects).
pub fn walk_value<V: CstVisitor>(visitor: &mut V, value: &cst::Value) {
    visitor.visit_value(value);

    match value {
        cst::Value::Variable(var) => {
            visitor.visit_variable(var);
        }
        cst::Value::ListValue(list) => {
            for item in list.values() {
                walk_value(visitor, &item);
            }
        }
        cst::Value::ObjectValue(obj) => {
            for field in obj.object_fields() {
                if let Some(val) = field.value() {
                    walk_value(visitor, &val);
                }
            }
        }
        _ => {}
    }
}

/// Walk a type reference.
pub fn walk_type<V: CstVisitor>(visitor: &mut V, ty: &cst::Type) {
    visitor.visit_type(ty);

    match ty {
        cst::Type::NamedType(named) => {
            visitor.visit_named_type(named);
        }
        cst::Type::ListType(list) => {
            if let Some(inner) = list.ty() {
                walk_type(visitor, &inner);
            }
        }
        cst::Type::NonNullType(non_null) => {
            if let Some(named) = non_null.named_type() {
                visitor.visit_named_type(&named);
            }
            if let Some(list) = non_null.list_type() {
                if let Some(inner) = list.ty() {
                    walk_type(visitor, &inner);
                }
            }
        }
    }
}

// =============================================================================
// Schema type walkers
// =============================================================================

fn walk_object_type<V: CstVisitor>(visitor: &mut V, obj: &cst::ObjectTypeDefinition) {
    visitor.visit_object_type(obj);

    if let Some(directives) = obj.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(fields) = obj.fields_definition() {
        walk_fields_definition(visitor, &fields);
    }
}

fn walk_interface_type<V: CstVisitor>(visitor: &mut V, iface: &cst::InterfaceTypeDefinition) {
    visitor.visit_interface_type(iface);

    if let Some(directives) = iface.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(fields) = iface.fields_definition() {
        walk_fields_definition(visitor, &fields);
    }
}

fn walk_union_type<V: CstVisitor>(visitor: &mut V, union: &cst::UnionTypeDefinition) {
    visitor.visit_union_type(union);

    if let Some(directives) = union.directives() {
        walk_directives(visitor, &directives);
    }
}

fn walk_enum_type<V: CstVisitor>(visitor: &mut V, enum_def: &cst::EnumTypeDefinition) {
    visitor.visit_enum_type(enum_def);

    if let Some(directives) = enum_def.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(values) = enum_def.enum_values_definition() {
        for value in values.enum_value_definitions() {
            visitor.visit_enum_value_definition(&value);
            if let Some(directives) = value.directives() {
                walk_directives(visitor, &directives);
            }
        }
    }
}

fn walk_scalar_type<V: CstVisitor>(visitor: &mut V, scalar: &cst::ScalarTypeDefinition) {
    visitor.visit_scalar_type(scalar);

    if let Some(directives) = scalar.directives() {
        walk_directives(visitor, &directives);
    }
}

fn walk_input_object_type<V: CstVisitor>(visitor: &mut V, input: &cst::InputObjectTypeDefinition) {
    visitor.visit_input_object_type(input);

    if let Some(directives) = input.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(fields) = input.input_fields_definition() {
        walk_input_fields_definition(visitor, &fields);
    }
}

fn walk_directive_definition<V: CstVisitor>(visitor: &mut V, dir: &cst::DirectiveDefinition) {
    visitor.visit_directive_definition(dir);

    if let Some(args) = dir.arguments_definition() {
        walk_arguments_definition(visitor, &args);
    }
}

fn walk_schema_definition<V: CstVisitor>(visitor: &mut V, schema: &cst::SchemaDefinition) {
    visitor.visit_schema_definition(schema);

    if let Some(directives) = schema.directives() {
        walk_directives(visitor, &directives);
    }
}

fn walk_fields_definition<V: CstVisitor>(visitor: &mut V, fields: &cst::FieldsDefinition) {
    for field in fields.field_definitions() {
        visitor.visit_field_definition(&field);

        if let Some(args) = field.arguments_definition() {
            walk_arguments_definition(visitor, &args);
        }

        if let Some(ty) = field.ty() {
            walk_type(visitor, &ty);
        }

        if let Some(directives) = field.directives() {
            walk_directives(visitor, &directives);
        }
    }
}

fn walk_input_fields_definition<V: CstVisitor>(
    visitor: &mut V,
    fields: &cst::InputFieldsDefinition,
) {
    for field in fields.input_value_definitions() {
        visitor.visit_input_value_definition(&field);

        if let Some(ty) = field.ty() {
            walk_type(visitor, &ty);
        }

        if let Some(directives) = field.directives() {
            walk_directives(visitor, &directives);
        }
    }
}

fn walk_arguments_definition<V: CstVisitor>(visitor: &mut V, args: &cst::ArgumentsDefinition) {
    for arg in args.input_value_definitions() {
        visitor.visit_input_value_definition(&arg);

        if let Some(ty) = arg.ty() {
            walk_type(visitor, &ty);
        }

        if let Some(directives) = arg.directives() {
            walk_directives(visitor, &directives);
        }
    }
}

// =============================================================================
// Extension walkers
// =============================================================================

fn walk_object_type_extension<V: CstVisitor>(visitor: &mut V, ext: &cst::ObjectTypeExtension) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(fields) = ext.fields_definition() {
        walk_fields_definition(visitor, &fields);
    }
}

fn walk_interface_type_extension<V: CstVisitor>(
    visitor: &mut V,
    ext: &cst::InterfaceTypeExtension,
) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(fields) = ext.fields_definition() {
        walk_fields_definition(visitor, &fields);
    }
}

fn walk_union_type_extension<V: CstVisitor>(visitor: &mut V, ext: &cst::UnionTypeExtension) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }
}

fn walk_enum_type_extension<V: CstVisitor>(visitor: &mut V, ext: &cst::EnumTypeExtension) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(values) = ext.enum_values_definition() {
        for value in values.enum_value_definitions() {
            visitor.visit_enum_value_definition(&value);
            if let Some(directives) = value.directives() {
                walk_directives(visitor, &directives);
            }
        }
    }
}

fn walk_scalar_type_extension<V: CstVisitor>(visitor: &mut V, ext: &cst::ScalarTypeExtension) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }
}

fn walk_input_object_type_extension<V: CstVisitor>(
    visitor: &mut V,
    ext: &cst::InputObjectTypeExtension,
) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }

    if let Some(fields) = ext.input_fields_definition() {
        walk_input_fields_definition(visitor, &fields);
    }
}

fn walk_schema_extension<V: CstVisitor>(visitor: &mut V, ext: &cst::SchemaExtension) {
    if let Some(directives) = ext.directives() {
        walk_directives(visitor, &directives);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_visitor() {
        struct FieldCollector(Vec<String>);

        impl CstVisitor for FieldCollector {
            fn visit_field(&mut self, field: &cst::Field) {
                if let Some(name) = field.name() {
                    self.0.push(name.text().to_string());
                }
            }
        }

        let source = "query { user { name email } }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut collector = FieldCollector(vec![]);
        walk_document(&mut collector, &tree);

        assert_eq!(collector.0, vec!["user", "name", "email"]);
    }

    #[test]
    fn test_fragment_spread_visitor() {
        struct FragmentCollector(Vec<String>);

        impl CstVisitor for FragmentCollector {
            fn visit_fragment_spread(&mut self, spread: &cst::FragmentSpread) {
                if let Some(name) = spread.fragment_name().and_then(|n| n.name()) {
                    self.0.push(name.text().to_string());
                }
            }
        }

        let source = "query { ...UserFields user { ...NameFields } }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut collector = FragmentCollector(vec![]);
        walk_document(&mut collector, &tree);

        assert_eq!(collector.0, vec!["UserFields", "NameFields"]);
    }

    #[test]
    fn test_variable_visitor() {
        struct VariableCollector(Vec<String>);

        impl CstVisitor for VariableCollector {
            fn visit_variable(&mut self, var: &cst::Variable) {
                if let Some(name) = var.name() {
                    self.0.push(name.text().to_string());
                }
            }
        }

        let source = "query($id: ID!, $name: String) { user(id: $id, name: $name) { id } }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut collector = VariableCollector(vec![]);
        walk_document(&mut collector, &tree);

        // Variables in definitions and usages
        assert_eq!(collector.0, vec!["id", "name", "id", "name"]);
    }

    #[test]
    fn test_type_definition_visitor() {
        struct TypeCollector(Vec<String>);

        impl CstVisitor for TypeCollector {
            fn visit_object_type(&mut self, obj: &cst::ObjectTypeDefinition) {
                if let Some(name) = obj.name() {
                    self.0.push(name.text().to_string());
                }
            }

            fn visit_interface_type(&mut self, iface: &cst::InterfaceTypeDefinition) {
                if let Some(name) = iface.name() {
                    self.0.push(name.text().to_string());
                }
            }
        }

        let source = "type User { id: ID! } interface Node { id: ID! }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut collector = TypeCollector(vec![]);
        walk_document(&mut collector, &tree);

        assert_eq!(collector.0, vec!["User", "Node"]);
    }

    #[test]
    fn test_inline_fragment_visitor() {
        struct TypeConditionCollector(Vec<String>);

        impl CstVisitor for TypeConditionCollector {
            fn enter_inline_fragment(&mut self, inline: &cst::InlineFragment) {
                if let Some(tc) = inline.type_condition() {
                    if let Some(named) = tc.named_type() {
                        if let Some(name) = named.name() {
                            self.0.push(name.text().to_string());
                        }
                    }
                }
            }
        }

        let source = "query { ... on User { name } ... on Admin { role } }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut collector = TypeConditionCollector(vec![]);
        walk_document(&mut collector, &tree);

        assert_eq!(collector.0, vec!["User", "Admin"]);
    }

    #[test]
    fn test_directive_visitor() {
        struct DirectiveCollector(Vec<String>);

        impl CstVisitor for DirectiveCollector {
            fn visit_directive(&mut self, dir: &cst::Directive) {
                if let Some(name) = dir.name() {
                    self.0.push(name.text().to_string());
                }
            }
        }

        let source = "query @cached { user @skip(if: true) { name @deprecated } }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut collector = DirectiveCollector(vec![]);
        walk_document(&mut collector, &tree);

        assert_eq!(collector.0, vec!["cached", "skip", "deprecated"]);
    }

    #[test]
    fn test_nested_selection_sets() {
        struct DepthTracker {
            current_depth: usize,
            max_depth: usize,
        }

        impl CstVisitor for DepthTracker {
            fn enter_selection_set(&mut self, _set: &cst::SelectionSet) {
                self.current_depth += 1;
                self.max_depth = self.max_depth.max(self.current_depth);
            }

            fn exit_selection_set(&mut self, _set: &cst::SelectionSet) {
                self.current_depth -= 1;
            }
        }

        let source = "query { user { posts { comments { author { name } } } } }";
        let tree = apollo_parser::Parser::new(source).parse();
        let mut tracker = DepthTracker {
            current_depth: 0,
            max_depth: 0,
        };
        walk_document(&mut tracker, &tree);

        assert_eq!(tracker.max_depth, 5);
        assert_eq!(tracker.current_depth, 0);
    }
}
