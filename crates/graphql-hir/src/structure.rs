// Structure extraction - extracts names and signatures, not bodies
// This is the foundation of the golden invariant: structure is stable across body edits

use apollo_compiler::ast;
use apollo_compiler::Node;
use graphql_db::FileId;
use std::sync::Arc;
use text_size::{TextRange, TextSize};

/// Structure of a type definition (no field bodies)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDef {
    pub name: Arc<str>,
    pub kind: TypeDefKind,
    pub fields: Vec<FieldSignature>,
    pub implements: Vec<Arc<str>>,
    pub union_members: Vec<Arc<str>>,
    pub enum_values: Vec<EnumValue>,
    pub description: Option<Arc<str>>,
    pub file_id: FileId,
    /// The text range of the type name
    pub name_range: TextRange,
    /// The text range of the entire type definition
    pub definition_range: TextRange,
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
    pub is_deprecated: bool,
    pub deprecation_reason: Option<Arc<str>>,
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
    pub is_deprecated: bool,
    pub deprecation_reason: Option<Arc<str>>,
}

/// Enum value definition
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumValue {
    pub name: Arc<str>,
    pub description: Option<Arc<str>>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<Arc<str>>,
}

/// Operation structure (name and variables, no selection set details)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationStructure {
    pub name: Option<Arc<str>>,
    pub operation_type: OperationType,
    pub variables: Vec<VariableSignature>,
    pub file_id: FileId,
    pub index: usize,
    /// The text range of the operation name (if named)
    pub name_range: Option<TextRange>,
    /// The text range of the entire operation
    pub operation_range: TextRange,
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
    /// The text range of the fragment name
    pub name_range: TextRange,
    /// The text range of the type condition
    pub type_condition_range: TextRange,
    /// The text range of the entire fragment definition
    pub fragment_range: TextRange,
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

/// Extract a `TextRange` from an apollo-compiler `Node`
#[allow(clippy::cast_possible_truncation)]
fn node_range<T>(node: &Node<T>) -> TextRange {
    node.location()
        .map(|loc| {
            // Safe: GraphQL files are never larger than 4GB
            TextRange::new(
                TextSize::from(loc.offset() as u32),
                TextSize::from(loc.end_offset() as u32),
            )
        })
        .unwrap_or_default()
}

/// Extract a `TextRange` from an apollo-compiler `Name`
#[allow(clippy::cast_possible_truncation)]
fn name_range(name: &apollo_compiler::Name) -> TextRange {
    name.location()
        .map(|loc| {
            // Safe: GraphQL files are never larger than 4GB
            TextRange::new(
                TextSize::from(loc.offset() as u32),
                TextSize::from(loc.end_offset() as u32),
            )
        })
        .unwrap_or_default()
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

    // For pure GraphQL files, extract from main AST
    // For TypeScript/JavaScript files, only extract from blocks to avoid double-counting
    if parse.blocks.is_empty() {
        extract_from_document(
            &parse.ast,
            file_id,
            &mut type_defs,
            &mut operations,
            &mut fragments,
        );
    } else {
        // Extract from extracted blocks (TypeScript/JavaScript)
        for (block_idx, block) in parse.blocks.iter().enumerate() {
            extract_from_document(
                &block.ast,
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
    }

    Arc::new(FileStructureData {
        file_id,
        type_defs,
        operations,
        fragments,
    })
}

fn extract_from_document(
    document: &ast::Document,
    file_id: FileId,
    type_defs: &mut Vec<TypeDef>,
    operations: &mut Vec<OperationStructure>,
    fragments: &mut Vec<FragmentStructure>,
) {
    for definition in &document.definitions {
        match definition {
            ast::Definition::OperationDefinition(op) => {
                operations.push(extract_operation_structure(op, file_id, operations.len()));
            }
            ast::Definition::FragmentDefinition(frag) => {
                fragments.push(extract_fragment_structure(frag, file_id));
            }
            ast::Definition::ObjectTypeDefinition(obj) => {
                type_defs.push(extract_object_type(obj, file_id));
            }
            ast::Definition::InterfaceTypeDefinition(iface) => {
                type_defs.push(extract_interface_type(iface, file_id));
            }
            ast::Definition::UnionTypeDefinition(union_def) => {
                type_defs.push(extract_union_type(union_def, file_id));
            }
            ast::Definition::EnumTypeDefinition(enum_def) => {
                type_defs.push(extract_enum_type(enum_def, file_id));
            }
            ast::Definition::ScalarTypeDefinition(scalar) => {
                type_defs.push(extract_scalar_type(scalar, file_id));
            }
            ast::Definition::InputObjectTypeDefinition(input) => {
                type_defs.push(extract_input_object_type(input, file_id));
            }
            _ => {}
        }
    }
}

fn extract_operation_structure(
    op: &Node<ast::OperationDefinition>,
    file_id: FileId,
    index: usize,
) -> OperationStructure {
    let name = op.name.as_ref().map(|n| Arc::from(n.as_str()));

    let operation_type = match op.operation_type {
        ast::OperationType::Query => OperationType::Query,
        ast::OperationType::Mutation => OperationType::Mutation,
        ast::OperationType::Subscription => OperationType::Subscription,
    };

    let variables = op
        .variables
        .iter()
        .map(|v| extract_variable_signature(v))
        .collect();

    // Extract name range from the Name if present
    let op_name_range = op.name.as_ref().map(name_range);

    OperationStructure {
        name,
        operation_type,
        variables,
        file_id,
        index,
        name_range: op_name_range,
        operation_range: node_range(op),
    }
}

fn extract_fragment_structure(
    frag: &Node<ast::FragmentDefinition>,
    file_id: FileId,
) -> FragmentStructure {
    let name = Arc::from(frag.name.as_str());
    let type_condition = Arc::from(frag.type_condition.as_str());

    // TODO: Extract actual positions from AST nodes
    FragmentStructure {
        name,
        type_condition,
        file_id,
        name_range: name_range(&frag.name),
        type_condition_range: name_range(&frag.type_condition),
        fragment_range: node_range(frag),
    }
}

fn extract_variable_signature(var: &ast::VariableDefinition) -> VariableSignature {
    let name = Arc::from(var.name.as_str());
    let type_ref = extract_type_ref(&var.ty);
    let default_value = var
        .default_value
        .as_ref()
        .map(|v| Arc::from(v.to_string().as_str()));

    VariableSignature {
        name,
        type_ref,
        default_value,
    }
}

fn extract_object_type(obj: &Node<ast::ObjectTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(obj.name.as_str());
    let description = obj.description.as_ref().map(|d| Arc::from(d.as_str()));

    let fields = obj
        .fields
        .iter()
        .map(|f| extract_field_signature(f))
        .collect();

    let implements = obj
        .implements_interfaces
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    // TODO: Extract actual positions from AST nodes
    TypeDef {
        name,
        kind: TypeDefKind::Object,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        file_id,
        name_range: name_range(&obj.name),
        definition_range: node_range(obj),
    }
}

fn extract_interface_type(iface: &Node<ast::InterfaceTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(iface.name.as_str());
    let description = iface.description.as_ref().map(|d| Arc::from(d.as_str()));

    let fields = iface
        .fields
        .iter()
        .map(|f| extract_field_signature(f))
        .collect();

    let implements = iface
        .implements_interfaces
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    // TODO: Extract actual positions from AST nodes
    TypeDef {
        name,
        kind: TypeDefKind::Interface,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        file_id,
        name_range: name_range(&iface.name),
        definition_range: node_range(iface),
    }
}

fn extract_union_type(union_def: &Node<ast::UnionTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(union_def.name.as_str());
    let description = union_def
        .description
        .as_ref()
        .map(|d| Arc::from(d.as_str()));

    let union_members = union_def
        .members
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    // TODO: Extract actual positions from AST nodes
    TypeDef {
        name,
        kind: TypeDefKind::Union,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members,
        enum_values: Vec::new(),
        description,
        file_id,
        name_range: name_range(&union_def.name),
        definition_range: node_range(union_def),
    }
}

fn extract_enum_type(enum_def: &Node<ast::EnumTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(enum_def.name.as_str());
    let description = enum_def.description.as_ref().map(|d| Arc::from(d.as_str()));

    let enum_values = enum_def
        .values
        .iter()
        .map(|v| {
            let (is_deprecated, deprecation_reason) = extract_deprecation(&v.directives);
            EnumValue {
                name: Arc::from(v.value.as_str()),
                description: v.description.as_ref().map(|d| Arc::from(d.as_str())),
                is_deprecated,
                deprecation_reason,
            }
        })
        .collect();

    // TODO: Extract actual positions from AST nodes
    TypeDef {
        name,
        kind: TypeDefKind::Enum,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values,
        description,
        file_id,
        name_range: name_range(&enum_def.name),
        definition_range: node_range(enum_def),
    }
}

fn extract_scalar_type(scalar: &Node<ast::ScalarTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(scalar.name.as_str());
    let description = scalar.description.as_ref().map(|d| Arc::from(d.as_str()));

    // TODO: Extract actual positions from AST nodes
    TypeDef {
        name,
        kind: TypeDefKind::Scalar,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        file_id,
        name_range: name_range(&scalar.name),
        definition_range: node_range(scalar),
    }
}

fn extract_input_object_type(
    input: &Node<ast::InputObjectTypeDefinition>,
    file_id: FileId,
) -> TypeDef {
    let name = Arc::from(input.name.as_str());
    let description = input.description.as_ref().map(|d| Arc::from(d.as_str()));

    let fields = input
        .fields
        .iter()
        .map(|f| extract_input_field_signature(f))
        .collect();

    // TODO: Extract actual positions from AST nodes
    TypeDef {
        name,
        kind: TypeDefKind::InputObject,
        fields,
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        file_id,
        name_range: name_range(&input.name),
        definition_range: node_range(input),
    }
}

fn extract_field_signature(field: &ast::FieldDefinition) -> FieldSignature {
    let name = Arc::from(field.name.as_str());
    let type_ref = extract_type_ref(&field.ty);
    let description = field.description.as_ref().map(|d| Arc::from(d.as_str()));

    let arguments = field
        .arguments
        .iter()
        .map(|a| extract_argument_def(a))
        .collect();

    let (is_deprecated, deprecation_reason) = extract_deprecation(&field.directives);

    FieldSignature {
        name,
        type_ref,
        arguments,
        description,
        is_deprecated,
        deprecation_reason,
    }
}

fn extract_input_field_signature(field: &ast::InputValueDefinition) -> FieldSignature {
    let name = Arc::from(field.name.as_str());
    let type_ref = extract_type_ref(&field.ty);
    let description = field.description.as_ref().map(|d| Arc::from(d.as_str()));

    let (is_deprecated, deprecation_reason) = extract_deprecation(&field.directives);

    FieldSignature {
        name,
        type_ref,
        arguments: Vec::new(),
        description,
        is_deprecated,
        deprecation_reason,
    }
}

fn extract_argument_def(arg: &ast::InputValueDefinition) -> ArgumentDef {
    let name = Arc::from(arg.name.as_str());
    let type_ref = extract_type_ref(&arg.ty);
    let default_value = arg
        .default_value
        .as_ref()
        .map(|v| Arc::from(v.to_string().as_str()));
    let description = arg.description.as_ref().map(|d| Arc::from(d.as_str()));

    let (is_deprecated, deprecation_reason) = extract_deprecation(&arg.directives);

    ArgumentDef {
        name,
        type_ref,
        default_value,
        description,
        is_deprecated,
        deprecation_reason,
    }
}

/// Extract deprecation information from directives
fn extract_deprecation(
    directives: &apollo_compiler::ast::DirectiveList,
) -> (bool, Option<Arc<str>>) {
    for directive in directives {
        if directive.name == "deprecated" {
            // Look for reason argument
            let reason = directive.arguments.iter().find_map(|arg| {
                if arg.name == "reason" {
                    // Extract string value
                    if let apollo_compiler::ast::Value::String(s) = &*arg.value {
                        Some(Arc::from(s.as_str()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            return (true, reason);
        }
    }
    (false, None)
}

fn extract_type_ref(ty: &ast::Type) -> TypeRef {
    let is_non_null = ty.is_non_null();
    let is_list = ty.is_list();

    // Get the innermost named type
    let name = Arc::from(ty.inner_named_type().as_str());

    // Check if inner type (inside list) is non-null
    // For [Type!]! we need to check if the type inside the list is non-null
    let inner_non_null = if is_list {
        // Strip outer non-null wrapper if present
        let inner = if is_non_null {
            // For [Type]! or [Type!]!, get the inner type after unwrapping outer non-null
            match ty {
                ast::Type::NonNullNamed(_) => {
                    return TypeRef {
                        name,
                        is_list: false,
                        is_non_null: true,
                        inner_non_null: false,
                    }
                }
                ast::Type::NonNullList(inner) | ast::Type::List(inner) => inner.as_ref(),
                ast::Type::Named(_) => ty,
            }
        } else {
            ty
        };

        // Now check if it's a list with non-null inner type
        matches!(inner, ast::Type::List(list) if matches!(list.as_ref(), ast::Type::NonNullNamed(_)))
    } else {
        false
    };

    TypeRef {
        name,
        is_list,
        is_non_null,
        inner_non_null,
    }
}
