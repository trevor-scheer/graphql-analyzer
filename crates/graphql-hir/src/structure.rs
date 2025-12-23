// Structure extraction - extracts names and signatures, not bodies
// This is the foundation of the golden invariant: structure is stable across body edits

use apollo_parser::ast::{self, AstNode};
use graphql_db::FileId;
use std::sync::Arc;

/// Structure of a type definition (no field bodies)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDef {
    pub name: Arc<str>,
    pub kind: TypeDefKind,
    pub fields: Vec<FieldSignature>,
    pub implements: Vec<Arc<str>>,
    pub union_members: Vec<Arc<str>>,
    pub enum_values: Vec<Arc<str>>,
    pub description: Option<Arc<str>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeDefKind {
    Object,
    Interface,
    Union,
    Enum,
    Scalar,
    InputObject,
}

/// Signature of a field (no resolver, no body)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldSignature {
    pub name: Arc<str>,
    pub type_ref: TypeRef,
    pub arguments: Vec<ArgumentDef>,
    pub description: Option<Arc<str>>,
}

/// Reference to a type (with list/non-null wrappers)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef {
    pub name: Arc<str>,
    pub is_list: bool,
    pub is_non_null: bool,
    pub inner_non_null: bool,
}

/// Argument definition
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgumentDef {
    pub name: Arc<str>,
    pub type_ref: TypeRef,
    pub default_value: Option<Arc<str>>,
    pub description: Option<Arc<str>>,
}

/// Operation structure (name and variables, no selection set details)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationStructure {
    pub name: Option<Arc<str>>,
    pub operation_type: OperationType,
    pub variables: Vec<VariableSignature>,
    pub file_id: FileId,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

/// Variable signature
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableSignature {
    pub name: Arc<str>,
    pub type_ref: TypeRef,
    pub default_value: Option<Arc<str>>,
}

/// Fragment structure (name and type, no selection set details)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentStructure {
    pub name: Arc<str>,
    pub type_condition: Arc<str>,
    pub file_id: FileId,
}

/// Extract the file structure from a parsed syntax tree
/// This only extracts structural information (names, signatures), not bodies
#[salsa::tracked]
pub fn file_structure(db: &dyn crate::GraphQLHirDatabase, file_id: FileId) -> crate::FileStructure {
    let parse = graphql_syntax::parse(db, file_id);

    let mut type_defs = Vec::new();
    let mut operations = Vec::new();
    let mut fragments = Vec::new();

    // Extract from main tree
    extract_from_tree(
        &parse.tree,
        file_id,
        &mut type_defs,
        &mut operations,
        &mut fragments,
    );

    // Extract from extracted blocks (TypeScript/JavaScript)
    for (block_idx, block) in parse.blocks.iter().enumerate() {
        extract_from_tree(
            &block.tree,
            file_id,
            &mut type_defs,
            &mut operations,
            &mut fragments,
        );
        // Update operation indices to be unique per block
        for op in operations
            .iter_mut()
            .skip(operations.len().saturating_sub(1))
        {
            op.index += block_idx * 1000; // Simple offset to make unique
        }
    }

    crate::FileStructure::new(db, file_id, type_defs, operations, fragments)
}

fn extract_from_tree(
    tree: &apollo_parser::SyntaxTree,
    file_id: FileId,
    type_defs: &mut Vec<TypeDef>,
    operations: &mut Vec<OperationStructure>,
    fragments: &mut Vec<FragmentStructure>,
) {
    let document = tree.document();

    for definition in document.definitions() {
        match definition {
            ast::Definition::OperationDefinition(op) => {
                operations.push(extract_operation_structure(op, file_id, operations.len()));
            }
            ast::Definition::FragmentDefinition(frag) => {
                if let Some(fragment_struct) = extract_fragment_structure(frag, file_id) {
                    fragments.push(fragment_struct);
                }
            }
            ast::Definition::SchemaDefinition(_) => {
                // Schema definition doesn't add types, just configures root types
            }
            ast::Definition::SchemaExtension(_) => {
                // Schema extension doesn't add types
            }
            ast::Definition::DirectiveDefinition(_) => {
                // Directive definitions not yet supported
            }
            _ => {
                // Type definitions
                if let Some(type_def) = extract_type_def(definition) {
                    type_defs.push(type_def);
                }
            }
        }
    }
}

fn extract_operation_structure(
    op: ast::OperationDefinition,
    file_id: FileId,
    index: usize,
) -> OperationStructure {
    let name = op.name().map(|n| Arc::from(n.text().as_str()));

    let operation_type = match op.operation_type() {
        Some(ast::OperationType::Query) | None => OperationType::Query,
        Some(ast::OperationType::Mutation) => OperationType::Mutation,
        Some(ast::OperationType::Subscription) => OperationType::Subscription,
    };

    let variables = op
        .variable_definitions()
        .into_iter()
        .flat_map(|vars| vars.variable_definitions())
        .filter_map(extract_variable_signature)
        .collect();

    OperationStructure {
        name,
        operation_type,
        variables,
        file_id,
        index,
    }
}

fn extract_fragment_structure(
    frag: ast::FragmentDefinition,
    file_id: FileId,
) -> Option<FragmentStructure> {
    let name = Arc::from(frag.fragment_name()?.name()?.text().as_str());
    let type_condition = Arc::from(frag.type_condition()?.named_type()?.name()?.text().as_str());

    Some(FragmentStructure {
        name,
        type_condition,
        file_id,
    })
}

fn extract_variable_signature(var: ast::VariableDefinition) -> Option<VariableSignature> {
    let name = Arc::from(var.variable()?.name()?.text().as_str());
    let type_ref = extract_type_ref(&var.ty()?)?;
    let default_value = var
        .default_value()
        .map(|v| Arc::from(v.value()?.to_string().as_str()));

    Some(VariableSignature {
        name,
        type_ref,
        default_value,
    })
}

fn extract_type_def(definition: ast::Definition) -> Option<TypeDef> {
    match definition {
        ast::Definition::ObjectTypeDefinition(obj) => extract_object_type(obj),
        ast::Definition::ObjectTypeExtension(obj) => extract_object_type_extension(obj),
        ast::Definition::InterfaceTypeDefinition(iface) => extract_interface_type(iface),
        ast::Definition::InterfaceTypeExtension(iface) => extract_interface_type_extension(iface),
        ast::Definition::UnionTypeDefinition(union) => extract_union_type(union),
        ast::Definition::UnionTypeExtension(union) => extract_union_type_extension(union),
        ast::Definition::EnumTypeDefinition(enum_def) => extract_enum_type(enum_def),
        ast::Definition::EnumTypeExtension(enum_def) => extract_enum_type_extension(enum_def),
        ast::Definition::ScalarTypeDefinition(scalar) => extract_scalar_type(scalar),
        ast::Definition::ScalarTypeExtension(scalar) => extract_scalar_type_extension(scalar),
        ast::Definition::InputObjectTypeDefinition(input) => extract_input_object_type(input),
        ast::Definition::InputObjectTypeExtension(input) => {
            extract_input_object_type_extension(input)
        }
        _ => None,
    }
}

fn extract_object_type(obj: ast::ObjectTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(obj.name()?.text().as_str());
    let description = obj
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    let fields = obj
        .fields_definition()?
        .field_definitions()
        .filter_map(extract_field_signature)
        .collect();

    let implements = obj
        .implements_interfaces()
        .into_iter()
        .flat_map(|impls| impls.named_types())
        .filter_map(|t| t.name().map(|n| Arc::from(n.text().as_str())))
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Object,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
    })
}

fn extract_object_type_extension(obj: ast::ObjectTypeExtension) -> Option<TypeDef> {
    let name = Arc::from(obj.name()?.text().as_str());

    let fields = obj
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
        .filter_map(extract_field_signature)
        .collect();

    let implements = obj
        .implements_interfaces()
        .into_iter()
        .flat_map(|impls| impls.named_types())
        .filter_map(|t| t.name().map(|n| Arc::from(n.text().as_str())))
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Object,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
    })
}

fn extract_interface_type(iface: ast::InterfaceTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(iface.name()?.text().as_str());
    let description = iface
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    let fields = iface
        .fields_definition()?
        .field_definitions()
        .filter_map(extract_field_signature)
        .collect();

    let implements = iface
        .implements_interfaces()
        .into_iter()
        .flat_map(|impls| impls.named_types())
        .filter_map(|t| t.name().map(|n| Arc::from(n.text().as_str())))
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Interface,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
    })
}

fn extract_interface_type_extension(iface: ast::InterfaceTypeExtension) -> Option<TypeDef> {
    let name = Arc::from(iface.name()?.text().as_str());

    let fields = iface
        .fields_definition()
        .into_iter()
        .flat_map(|f| f.field_definitions())
        .filter_map(extract_field_signature)
        .collect();

    let implements = iface
        .implements_interfaces()
        .into_iter()
        .flat_map(|impls| impls.named_types())
        .filter_map(|t| t.name().map(|n| Arc::from(n.text().as_str())))
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Interface,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
    })
}

fn extract_union_type(union: ast::UnionTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(union.name()?.text().as_str());
    let description = union
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    let union_members = union
        .union_member_types()?
        .named_types()
        .filter_map(|t| t.name().map(|n| Arc::from(n.text().as_str())))
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Union,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members,
        enum_values: Vec::new(),
        description,
    })
}

fn extract_union_type_extension(union: ast::UnionTypeExtension) -> Option<TypeDef> {
    let name = Arc::from(union.name()?.text().as_str());

    let union_members = union
        .union_member_types()
        .into_iter()
        .flat_map(|m| m.named_types())
        .filter_map(|t| t.name().map(|n| Arc::from(n.text().as_str())))
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Union,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members,
        enum_values: Vec::new(),
        description: None,
    })
}

fn extract_enum_type(enum_def: ast::EnumTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(enum_def.name()?.text().as_str());
    let description = enum_def
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    let enum_values = enum_def
        .enum_values_definition()?
        .enum_value_definitions()
        .filter_map(|v| {
            v.enum_value()
                .and_then(|e| e.name())
                .map(|n| Arc::from(n.text().as_str()))
        })
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Enum,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values,
        description,
    })
}

fn extract_enum_type_extension(enum_def: ast::EnumTypeExtension) -> Option<TypeDef> {
    let name = Arc::from(enum_def.name()?.text().as_str());

    let enum_values = enum_def
        .enum_values_definition()
        .into_iter()
        .flat_map(|e| e.enum_value_definitions())
        .filter_map(|v| {
            v.enum_value()
                .and_then(|e| e.name())
                .map(|n| Arc::from(n.text().as_str()))
        })
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::Enum,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values,
        description: None,
    })
}

fn extract_scalar_type(scalar: ast::ScalarTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(scalar.name()?.text().as_str());
    let description = scalar
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    Some(TypeDef {
        name,
        kind: TypeDefKind::Scalar,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
    })
}

fn extract_scalar_type_extension(scalar: ast::ScalarTypeExtension) -> Option<TypeDef> {
    let name = Arc::from(scalar.name()?.text().as_str());

    Some(TypeDef {
        name,
        kind: TypeDefKind::Scalar,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
    })
}

fn extract_input_object_type(input: ast::InputObjectTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(input.name()?.text().as_str());
    let description = input
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    let fields = input
        .input_fields_definition()?
        .input_value_definitions()
        .filter_map(extract_input_field_signature)
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::InputObject,
        fields,
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
    })
}

fn extract_input_object_type_extension(input: ast::InputObjectTypeExtension) -> Option<TypeDef> {
    let name = Arc::from(input.name()?.text().as_str());

    let fields = input
        .input_fields_definition()
        .into_iter()
        .flat_map(|f| f.input_value_definitions())
        .filter_map(extract_input_field_signature)
        .collect();

    Some(TypeDef {
        name,
        kind: TypeDefKind::InputObject,
        fields,
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
    })
}

fn extract_field_signature(field: ast::FieldDefinition) -> Option<FieldSignature> {
    let name = Arc::from(field.name()?.text().as_str());
    let type_ref = extract_type_ref(&field.ty()?)?;
    let description = field
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    let arguments = field
        .arguments_definition()
        .into_iter()
        .flat_map(|args| args.input_value_definitions())
        .filter_map(extract_argument_def)
        .collect();

    Some(FieldSignature {
        name,
        type_ref,
        arguments,
        description,
    })
}

fn extract_input_field_signature(field: ast::InputValueDefinition) -> Option<FieldSignature> {
    let name = Arc::from(field.name()?.text().as_str());
    let type_ref = extract_type_ref(&field.ty()?)?;
    let description = field
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    Some(FieldSignature {
        name,
        type_ref,
        arguments: Vec::new(),
        description,
    })
}

fn extract_argument_def(arg: ast::InputValueDefinition) -> Option<ArgumentDef> {
    let name = Arc::from(arg.name()?.text().as_str());
    let type_ref = extract_type_ref(&arg.ty()?)?;
    let default_value = arg
        .default_value()
        .map(|v| Arc::from(v.value()?.to_string().as_str()));
    let description = arg
        .description()
        .map(|d| Arc::from(d.to_string().trim_matches('"').as_str()));

    Some(ArgumentDef {
        name,
        type_ref,
        default_value,
        description,
    })
}

fn extract_type_ref(ty: &ast::Type) -> Option<TypeRef> {
    match ty {
        ast::Type::NamedType(named) => {
            let name = Arc::from(named.name()?.text().as_str());
            Some(TypeRef {
                name,
                is_list: false,
                is_non_null: false,
                inner_non_null: false,
            })
        }
        ast::Type::NonNullType(non_null) => {
            let inner = non_null.ty()?;
            match inner {
                ast::Type::NamedType(named) => {
                    let name = Arc::from(named.name()?.text().as_str());
                    Some(TypeRef {
                        name,
                        is_list: false,
                        is_non_null: true,
                        inner_non_null: false,
                    })
                }
                ast::Type::ListType(list) => {
                    let inner_type = list.ty()?;
                    if let ast::Type::NamedType(named) = inner_type {
                        let name = Arc::from(named.name()?.text().as_str());
                        Some(TypeRef {
                            name,
                            is_list: true,
                            is_non_null: true,
                            inner_non_null: false,
                        })
                    } else if let ast::Type::NonNullType(inner_non_null) = inner_type {
                        if let Some(ast::Type::NamedType(named)) = inner_non_null.ty() {
                            let name = Arc::from(named.name()?.text().as_str());
                            Some(TypeRef {
                                name,
                                is_list: true,
                                is_non_null: true,
                                inner_non_null: true,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        ast::Type::ListType(list) => {
            let inner = list.ty()?;
            match inner {
                ast::Type::NamedType(named) => {
                    let name = Arc::from(named.name()?.text().as_str());
                    Some(TypeRef {
                        name,
                        is_list: true,
                        is_non_null: false,
                        inner_non_null: false,
                    })
                }
                ast::Type::NonNullType(non_null) => {
                    if let Some(ast::Type::NamedType(named)) = non_null.ty() {
                        let name = Arc::from(named.name()?.text().as_str());
                        Some(TypeRef {
                            name,
                            is_list: true,
                            is_non_null: false,
                            inner_non_null: true,
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }
}
