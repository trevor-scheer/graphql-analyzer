use apollo_compiler::ast;
use apollo_compiler::Node;
use graphql_base_db::FileId;
use std::sync::Arc;
pub use text_size::{TextRange, TextSize};

/// Offset multiplier to ensure unique operation indices across blocks.
/// Each block's operations get offset by `block_index * BLOCK_INDEX_OFFSET`.
const BLOCK_INDEX_OFFSET: usize = 1000;

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
    pub directives: Vec<DirectiveUsage>,
    pub file_id: FileId,
    /// The text range of the type name
    pub name_range: TextRange,
    /// The text range of the entire type definition
    pub definition_range: TextRange,
    /// Whether this type was extracted from a type extension (extend type)
    pub is_extension: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
    pub directives: Vec<DirectiveUsage>,
    /// The text range of the field name
    pub name_range: TextRange,
    /// The file that contains this field definition (needed for extension types
    /// where a field may come from a different file than the base type)
    pub file_id: FileId,
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
    pub directives: Vec<DirectiveUsage>,
    /// The text range of the argument name
    pub name_range: TextRange,
}

/// Enum value definition
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumValue {
    pub name: Arc<str>,
    pub description: Option<Arc<str>>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<Arc<str>>,
    pub directives: Vec<DirectiveUsage>,
}

/// A directive applied to a schema element
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DirectiveUsage {
    pub name: Arc<str>,
    pub arguments: Vec<DirectiveArgument>,
}

/// An argument passed to a directive
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DirectiveArgument {
    pub name: Arc<str>,
    /// Serialized value (e.g. `"hello"`, `true`, `ENUM_VALUE`)
    pub value: Arc<str>,
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
    /// For embedded GraphQL: line offset of the block (0-indexed)
    pub block_line_offset: Option<u32>,
    /// For embedded GraphQL: byte offset of the block in the original file
    pub block_byte_offset: Option<usize>,
    /// For embedded GraphQL: source text of the block
    pub block_source: Option<Arc<str>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
    /// For embedded GraphQL: line offset of the block (0-indexed)
    pub block_line_offset: Option<u32>,
    /// For embedded GraphQL: byte offset of the block in the original file
    pub block_byte_offset: Option<usize>,
    /// For embedded GraphQL: source text of the block
    pub block_source: Option<Arc<str>>,
}

/// Summary of a file's structure (stable across body edits)
/// Contains extracted names and signatures, but not bodies.
///
/// Fields use `Arc<Vec<...>>` to enable cheap cloning without copying data.
/// This is critical for performance: queries like `file_fragments` can return
/// a clone of the Arc instead of cloning the entire vector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileStructureData {
    pub file_id: FileId,
    pub type_defs: Arc<Vec<TypeDef>>,
    pub operations: Arc<Vec<OperationStructure>>,
    pub fragments: Arc<Vec<FragmentStructure>>,
}

/// Extract a `TextRange` from an apollo-compiler `Node`
fn node_range<T>(node: &Node<T>) -> TextRange {
    node.location()
        .map(|loc| {
            TextRange::new(
                TextSize::from(loc.offset() as u32),
                TextSize::from(loc.end_offset() as u32),
            )
        })
        .unwrap_or_default()
}

/// Extract a `TextRange` from an apollo-compiler `Name`
fn name_range(name: &apollo_compiler::Name) -> TextRange {
    name.location()
        .map(|loc| {
            TextRange::new(
                TextSize::from(loc.offset() as u32),
                TextSize::from(loc.end_offset() as u32),
            )
        })
        .unwrap_or_default()
}

/// Block context for embedded GraphQL extraction
#[derive(Debug, Clone)]
struct BlockContext {
    /// Line offset in the original file (0-indexed)
    line_offset: u32,
    /// Byte offset in the original file
    byte_offset: usize,
    /// Source text of the block
    source: Option<Arc<str>>,
}

impl BlockContext {
    /// Create a new block context for pure GraphQL files (no offset, no source)
    const fn pure_graphql() -> Self {
        Self {
            line_offset: 0,
            byte_offset: 0,
            source: None,
        }
    }

    /// Create a new block context for embedded GraphQL
    fn embedded(line_offset: u32, byte_offset: usize, source: Arc<str>) -> Self {
        Self {
            line_offset,
            byte_offset,
            source: Some(source),
        }
    }
}

/// Extract the file structure from a parsed syntax tree
/// This only extracts structural information (names, signatures), not bodies
#[salsa::tracked]
pub fn file_structure(
    db: &dyn crate::GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
) -> Arc<FileStructureData> {
    let parse = graphql_syntax::parse(db, content, metadata);

    let mut type_defs = Vec::new();
    let mut operations = Vec::new();
    let mut fragments = Vec::new();

    for (block_idx, doc) in parse.documents().enumerate() {
        // For embedded GraphQL (byte_offset > 0), include block context
        // For pure GraphQL (byte_offset == 0), no block context needed
        let block_ctx = if doc.byte_offset > 0 {
            BlockContext::embedded(doc.line_offset, doc.byte_offset, Arc::from(doc.source))
        } else {
            BlockContext::pure_graphql()
        };

        extract_from_document(
            doc.ast,
            file_id,
            &block_ctx,
            &mut type_defs,
            &mut operations,
            &mut fragments,
        );
        if block_idx > 0 {
            let ops_len = operations.len();
            for op in operations.iter_mut().skip(ops_len.saturating_sub(1)) {
                op.index += block_idx * BLOCK_INDEX_OFFSET;
            }
        }
    }

    Arc::new(FileStructureData {
        file_id,
        type_defs: Arc::new(type_defs),
        operations: Arc::new(operations),
        fragments: Arc::new(fragments),
    })
}

fn extract_from_document(
    document: &ast::Document,
    file_id: FileId,
    block_ctx: &BlockContext,
    type_defs: &mut Vec<TypeDef>,
    operations: &mut Vec<OperationStructure>,
    fragments: &mut Vec<FragmentStructure>,
) {
    for definition in &document.definitions {
        match definition {
            ast::Definition::OperationDefinition(op) => {
                operations.push(extract_operation_structure(
                    op,
                    file_id,
                    operations.len(),
                    block_ctx,
                ));
            }
            ast::Definition::FragmentDefinition(frag) => {
                fragments.push(extract_fragment_structure(frag, file_id, block_ctx));
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
            // Type extensions - these get merged with base types in schema_types()
            ast::Definition::ObjectTypeExtension(ext) => {
                type_defs.push(extract_object_type_extension(ext, file_id));
            }
            ast::Definition::InterfaceTypeExtension(ext) => {
                type_defs.push(extract_interface_type_extension(ext, file_id));
            }
            ast::Definition::UnionTypeExtension(ext) => {
                type_defs.push(extract_union_type_extension(ext, file_id));
            }
            ast::Definition::EnumTypeExtension(ext) => {
                type_defs.push(extract_enum_type_extension(ext, file_id));
            }
            ast::Definition::InputObjectTypeExtension(ext) => {
                type_defs.push(extract_input_object_type_extension(ext, file_id));
            }
            ast::Definition::ScalarTypeExtension(ext) => {
                type_defs.push(extract_scalar_type_extension(ext, file_id));
            }
            _ => {}
        }
    }
}

fn extract_operation_structure(
    op: &Node<ast::OperationDefinition>,
    file_id: FileId,
    index: usize,
    block_ctx: &BlockContext,
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

    let op_name_range = op.name.as_ref().map(name_range);

    // For embedded GraphQL, include block context; for pure GraphQL, these are None
    let (block_line_offset, block_byte_offset, block_source) = if block_ctx.source.is_some() {
        (
            Some(block_ctx.line_offset),
            Some(block_ctx.byte_offset),
            block_ctx.source.clone(),
        )
    } else {
        (None, None, None)
    };

    OperationStructure {
        name,
        operation_type,
        variables,
        file_id,
        index,
        name_range: op_name_range,
        operation_range: node_range(op),
        block_line_offset,
        block_byte_offset,
        block_source,
    }
}

fn extract_fragment_structure(
    frag: &Node<ast::FragmentDefinition>,
    file_id: FileId,
    block_ctx: &BlockContext,
) -> FragmentStructure {
    let name = Arc::from(frag.name.as_str());
    let type_condition = Arc::from(frag.type_condition.as_str());

    // For embedded GraphQL, include block context; for pure GraphQL, these are None
    let (block_line_offset, block_byte_offset, block_source) = if block_ctx.source.is_some() {
        (
            Some(block_ctx.line_offset),
            Some(block_ctx.byte_offset),
            block_ctx.source.clone(),
        )
    } else {
        (None, None, None)
    };

    FragmentStructure {
        name,
        type_condition,
        file_id,
        name_range: name_range(&frag.name),
        type_condition_range: name_range(&frag.type_condition),
        fragment_range: node_range(frag),
        block_line_offset,
        block_byte_offset,
        block_source,
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
        .map(|f| extract_field_signature(f, file_id))
        .collect();

    let implements = obj
        .implements_interfaces
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::Object,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        directives: extract_directives(&obj.directives),
        file_id,
        name_range: name_range(&obj.name),
        definition_range: node_range(obj),
        is_extension: false,
    }
}

fn extract_interface_type(iface: &Node<ast::InterfaceTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(iface.name.as_str());
    let description = iface.description.as_ref().map(|d| Arc::from(d.as_str()));

    let fields = iface
        .fields
        .iter()
        .map(|f| extract_field_signature(f, file_id))
        .collect();

    let implements = iface
        .implements_interfaces
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::Interface,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        directives: extract_directives(&iface.directives),
        file_id,
        name_range: name_range(&iface.name),
        definition_range: node_range(iface),
        is_extension: false,
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

    TypeDef {
        name,
        kind: TypeDefKind::Union,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members,
        enum_values: Vec::new(),
        description,
        directives: extract_directives(&union_def.directives),
        file_id,
        name_range: name_range(&union_def.name),
        definition_range: node_range(union_def),
        is_extension: false,
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
                directives: extract_directives(&v.directives),
            }
        })
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::Enum,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values,
        description,
        directives: extract_directives(&enum_def.directives),
        file_id,
        name_range: name_range(&enum_def.name),
        definition_range: node_range(enum_def),
        is_extension: false,
    }
}

fn extract_scalar_type(scalar: &Node<ast::ScalarTypeDefinition>, file_id: FileId) -> TypeDef {
    let name = Arc::from(scalar.name.as_str());
    let description = scalar.description.as_ref().map(|d| Arc::from(d.as_str()));

    TypeDef {
        name,
        kind: TypeDefKind::Scalar,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        directives: extract_directives(&scalar.directives),
        file_id,
        name_range: name_range(&scalar.name),
        definition_range: node_range(scalar),
        is_extension: false,
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
        .map(|f| extract_input_field_signature(f, file_id))
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::InputObject,
        fields,
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description,
        directives: extract_directives(&input.directives),
        file_id,
        name_range: name_range(&input.name),
        definition_range: node_range(input),
        is_extension: false,
    }
}

// =============================================================================
// Type Extension Extraction
// =============================================================================

fn extract_object_type_extension(ext: &Node<ast::ObjectTypeExtension>, file_id: FileId) -> TypeDef {
    let name = Arc::from(ext.name.as_str());

    let fields = ext
        .fields
        .iter()
        .map(|f| extract_field_signature(f, file_id))
        .collect();

    let implements = ext
        .implements_interfaces
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::Object,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
        directives: extract_directives(&ext.directives),
        file_id,
        name_range: name_range(&ext.name),
        definition_range: node_range(ext),
        is_extension: true,
    }
}

fn extract_interface_type_extension(
    ext: &Node<ast::InterfaceTypeExtension>,
    file_id: FileId,
) -> TypeDef {
    let name = Arc::from(ext.name.as_str());

    let fields = ext
        .fields
        .iter()
        .map(|f| extract_field_signature(f, file_id))
        .collect();

    let implements = ext
        .implements_interfaces
        .iter()
        .map(|t| Arc::from(t.as_str()))
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::Interface,
        fields,
        implements,
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
        directives: extract_directives(&ext.directives),
        file_id,
        name_range: name_range(&ext.name),
        definition_range: node_range(ext),
        is_extension: true,
    }
}

fn extract_union_type_extension(ext: &Node<ast::UnionTypeExtension>, file_id: FileId) -> TypeDef {
    let name = Arc::from(ext.name.as_str());

    let union_members = ext.members.iter().map(|t| Arc::from(t.as_str())).collect();

    TypeDef {
        name,
        kind: TypeDefKind::Union,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members,
        enum_values: Vec::new(),
        description: None,
        directives: extract_directives(&ext.directives),
        file_id,
        name_range: name_range(&ext.name),
        definition_range: node_range(ext),
        is_extension: true,
    }
}

fn extract_enum_type_extension(ext: &Node<ast::EnumTypeExtension>, file_id: FileId) -> TypeDef {
    let name = Arc::from(ext.name.as_str());

    let enum_values = ext
        .values
        .iter()
        .map(|v| {
            let (is_deprecated, deprecation_reason) = extract_deprecation(&v.directives);
            EnumValue {
                name: Arc::from(v.value.as_str()),
                description: v.description.as_ref().map(|d| Arc::from(d.as_str())),
                is_deprecated,
                deprecation_reason,
                directives: extract_directives(&v.directives),
            }
        })
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::Enum,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values,
        description: None,
        directives: extract_directives(&ext.directives),
        file_id,
        name_range: name_range(&ext.name),
        definition_range: node_range(ext),
        is_extension: true,
    }
}

fn extract_input_object_type_extension(
    ext: &Node<ast::InputObjectTypeExtension>,
    file_id: FileId,
) -> TypeDef {
    let name = Arc::from(ext.name.as_str());

    let fields = ext
        .fields
        .iter()
        .map(|f| extract_input_field_signature(f, file_id))
        .collect();

    TypeDef {
        name,
        kind: TypeDefKind::InputObject,
        fields,
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
        directives: extract_directives(&ext.directives),
        file_id,
        name_range: name_range(&ext.name),
        definition_range: node_range(ext),
        is_extension: true,
    }
}

fn extract_scalar_type_extension(ext: &Node<ast::ScalarTypeExtension>, file_id: FileId) -> TypeDef {
    let name = Arc::from(ext.name.as_str());

    TypeDef {
        name,
        kind: TypeDefKind::Scalar,
        fields: Vec::new(),
        implements: Vec::new(),
        union_members: Vec::new(),
        enum_values: Vec::new(),
        description: None,
        directives: extract_directives(&ext.directives),
        file_id,
        name_range: name_range(&ext.name),
        definition_range: node_range(ext),
        is_extension: true,
    }
}

fn extract_field_signature(field: &ast::FieldDefinition, file_id: FileId) -> FieldSignature {
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
        directives: extract_directives(&field.directives),
        name_range: name_range(&field.name),
        file_id,
    }
}

fn extract_input_field_signature(
    field: &ast::InputValueDefinition,
    file_id: FileId,
) -> FieldSignature {
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
        directives: extract_directives(&field.directives),
        name_range: name_range(&field.name),
        file_id,
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
        directives: extract_directives(&arg.directives),
        name_range: name_range(&arg.name),
    }
}

/// Extract deprecation information from directives
fn extract_deprecation(
    directives: &apollo_compiler::ast::DirectiveList,
) -> (bool, Option<Arc<str>>) {
    for directive in directives {
        if directive.name == "deprecated" {
            let reason = directive.arguments.iter().find_map(|arg| {
                if arg.name == "reason" {
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

/// Extract all directives from a directive list
fn extract_directives(directives: &apollo_compiler::ast::DirectiveList) -> Vec<DirectiveUsage> {
    directives
        .iter()
        .map(|directive| DirectiveUsage {
            name: Arc::from(directive.name.as_str()),
            arguments: directive
                .arguments
                .iter()
                .map(|arg| DirectiveArgument {
                    name: Arc::from(arg.name.as_str()),
                    value: Arc::from(arg.value.to_string().as_str()),
                })
                .collect(),
        })
        .collect()
}

fn extract_type_ref(ty: &ast::Type) -> TypeRef {
    let is_non_null = ty.is_non_null();
    let is_list = ty.is_list();

    let name = Arc::from(ty.inner_named_type().as_str());

    // For [Type!]! we need to check if the inner type is non-null
    let inner_non_null = if is_list {
        let inner = if is_non_null {
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
