// Structure extraction - extracts names and signatures, not bodies
// This is the foundation of the golden invariant: structure is stable across body edits

use apollo_parser::cst::{self, CstNode};
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

/// Summary of a file's structure (stable across body edits)
/// Contains extracted names and signatures, but not bodies
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileStructureData {
    pub file_id: FileId,
    pub type_defs: Vec<TypeDef>,
    pub operations: Vec<OperationStructure>,
    pub fragments: Vec<FragmentStructure>,
}

/// Extract the file structure from a parsed syntax tree
/// This only extracts structural information (names, signatures), not bodies
#[salsa::tracked]
pub fn file_structure(
    db: &dyn crate::GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<FileStructureData> {
    let parse = graphql_syntax::parse(db, content, metadata);

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
        let ops_len = operations.len();
        for op in operations.iter_mut().skip(ops_len.saturating_sub(1)) {
            op.index += block_idx * 1000; // Simple offset to make unique
        }
    }

    Arc::new(FileStructureData {
        file_id,
        type_defs,
        operations,
        fragments,
    })
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
            cst::Definition::OperationDefinition(op) => {
                operations.push(extract_operation_structure(&op, file_id, operations.len()));
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(fragment_struct) = extract_fragment_structure(&frag, file_id) {
                    fragments.push(fragment_struct);
                }
            }
            cst::Definition::ObjectTypeDefinition(obj) => {
                if let Some(type_def) = extract_object_type(&obj) {
                    type_defs.push(type_def);
                }
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                if let Some(type_def) = extract_interface_type(&iface) {
                    type_defs.push(type_def);
                }
            }
            cst::Definition::UnionTypeDefinition(union) => {
                if let Some(type_def) = extract_union_type(&union) {
                    type_defs.push(type_def);
                }
            }
            cst::Definition::EnumTypeDefinition(enum_def) => {
                if let Some(type_def) = extract_enum_type(&enum_def) {
                    type_defs.push(type_def);
                }
            }
            cst::Definition::ScalarTypeDefinition(scalar) => {
                if let Some(type_def) = extract_scalar_type(&scalar) {
                    type_defs.push(type_def);
                }
            }
            cst::Definition::InputObjectTypeDefinition(input) => {
                if let Some(type_def) = extract_input_object_type(&input) {
                    type_defs.push(type_def);
                }
            }
            _ => {}
        }
    }
}

fn extract_operation_structure(
    op: &cst::OperationDefinition,
    file_id: FileId,
    index: usize,
) -> OperationStructure {
    let name = op.name().map(|n| Arc::from(n.text().as_str()));

    let operation_type = op.operation_type().map_or(OperationType::Query, |op_type| {
        if op_type.query_token().is_some() {
            OperationType::Query
        } else if op_type.mutation_token().is_some() {
            OperationType::Mutation
        } else if op_type.subscription_token().is_some() {
            OperationType::Subscription
        } else {
            OperationType::Query
        }
    });

    let variables = op
        .variable_definitions()
        .into_iter()
        .flat_map(|vars| vars.variable_definitions())
        .filter_map(|var| extract_variable_signature(&var))
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
    frag: &cst::FragmentDefinition,
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

fn extract_variable_signature(var: &cst::VariableDefinition) -> Option<VariableSignature> {
    let name = Arc::from(var.variable()?.name()?.text().as_str());
    let type_ref = extract_type_ref(&var.ty()?)?;
    let default_value = var.default_value().and_then(|v| {
        v.value()
            .map(|val| Arc::from(val.syntax().text().to_string()))
    });

    Some(VariableSignature {
        name,
        type_ref,
        default_value,
    })
}

fn extract_object_type(obj: &cst::ObjectTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(obj.name()?.text().as_str());
    let description = obj
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

    let fields = obj
        .fields_definition()?
        .field_definitions()
        .filter_map(|f| extract_field_signature(&f))
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

fn extract_interface_type(iface: &cst::InterfaceTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(iface.name()?.text().as_str());
    let description = iface
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

    let fields = iface
        .fields_definition()?
        .field_definitions()
        .filter_map(|f| extract_field_signature(&f))
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

fn extract_union_type(union: &cst::UnionTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(union.name()?.text().as_str());
    let description = union
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

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

fn extract_enum_type(enum_def: &cst::EnumTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(enum_def.name()?.text().as_str());
    let description = enum_def
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

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

fn extract_scalar_type(scalar: &cst::ScalarTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(scalar.name()?.text().as_str());
    let description = scalar
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

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

fn extract_input_object_type(input: &cst::InputObjectTypeDefinition) -> Option<TypeDef> {
    let name = Arc::from(input.name()?.text().as_str());
    let description = input
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

    let fields = input
        .input_fields_definition()?
        .input_value_definitions()
        .filter_map(|f| extract_input_field_signature(&f))
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

fn extract_field_signature(field: &cst::FieldDefinition) -> Option<FieldSignature> {
    let name = Arc::from(field.name()?.text().as_str());
    let type_ref = extract_type_ref(&field.ty()?)?;
    let description = field
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

    let arguments = field
        .arguments_definition()
        .into_iter()
        .flat_map(|args| args.input_value_definitions())
        .filter_map(|a| extract_argument_def(&a))
        .collect();

    Some(FieldSignature {
        name,
        type_ref,
        arguments,
        description,
    })
}

fn extract_input_field_signature(field: &cst::InputValueDefinition) -> Option<FieldSignature> {
    let name = Arc::from(field.name()?.text().as_str());
    let type_ref = extract_type_ref(&field.ty()?)?;
    let description = field
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

    Some(FieldSignature {
        name,
        type_ref,
        arguments: Vec::new(),
        description,
    })
}

fn extract_argument_def(arg: &cst::InputValueDefinition) -> Option<ArgumentDef> {
    let name = Arc::from(arg.name()?.text().as_str());
    let type_ref = extract_type_ref(&arg.ty()?)?;
    let default_value = arg.default_value().and_then(|v| {
        v.value()
            .map(|val| Arc::from(val.syntax().text().to_string()))
    });
    let description = arg
        .description()
        .map(|d| Arc::from(d.syntax().text().to_string().trim_matches('"')));

    Some(ArgumentDef {
        name,
        type_ref,
        default_value,
        description,
    })
}

fn extract_type_ref(ty: &cst::Type) -> Option<TypeRef> {
    match ty {
        cst::Type::NamedType(named) => {
            let name = Arc::from(named.name()?.text().as_str());
            Some(TypeRef {
                name,
                is_list: false,
                is_non_null: false,
                inner_non_null: false,
            })
        }
        cst::Type::NonNullType(non_null) => {
            // NonNullType can contain either NamedType or ListType
            if let Some(named) = non_null.named_type() {
                // Type! case
                let name = Arc::from(named.name()?.text().as_str());
                Some(TypeRef {
                    name,
                    is_list: false,
                    is_non_null: true,
                    inner_non_null: false,
                })
            } else if let Some(list) = non_null.list_type() {
                // [Type]! or [Type!]! case
                let inner_type = list.ty()?;
                if let cst::Type::NamedType(named) = inner_type {
                    // [Type]! case
                    let name = Arc::from(named.name()?.text().as_str());
                    Some(TypeRef {
                        name,
                        is_list: true,
                        is_non_null: true,
                        inner_non_null: false,
                    })
                } else if let cst::Type::NonNullType(inner_non_null) = inner_type {
                    // [Type!]! case
                    if let Some(named) = inner_non_null.named_type() {
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
            } else {
                None
            }
        }
        cst::Type::ListType(list) => {
            let inner = list.ty()?;
            match inner {
                cst::Type::NamedType(named) => {
                    // [Type] case
                    let name = Arc::from(named.name()?.text().as_str());
                    Some(TypeRef {
                        name,
                        is_list: true,
                        is_non_null: false,
                        inner_non_null: false,
                    })
                }
                cst::Type::NonNullType(non_null) => {
                    // [Type!] case
                    if let Some(named) = non_null.named_type() {
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
                cst::Type::ListType(_) => None,
            }
        }
    }
}
