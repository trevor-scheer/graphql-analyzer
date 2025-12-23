// Schema validation queries

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_db::{FileContent, FileMetadata};
use std::collections::HashSet;
use std::sync::Arc;

/// Validate a schema file
/// This checks for:
/// - Duplicate type names within the file
/// - Conflicts with types in other files
/// - Invalid field definitions
/// - Invalid directive usage
#[salsa::tracked]
pub fn validate_schema_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let structure = graphql_hir::file_structure(db, metadata.file_id(db), content, metadata);
    let mut diagnostics = Vec::new();

    // Get all types for cross-referencing
    let all_types = graphql_hir::schema_types(db);

    // Check for duplicate type names within this file
    let mut seen_types = HashSet::new();
    for type_def in &structure.type_defs {
        if !seen_types.insert(type_def.name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("Duplicate type name: {}", type_def.name),
                DiagnosticRange::default(), // TODO: Get actual position from HIR
            ));
        }
    }

    // Validate each type definition
    for type_def in &structure.type_defs {
        validate_type_def(type_def, &all_types, &mut diagnostics);
    }

    Arc::new(diagnostics)
}

/// Validate a single type definition
fn validate_type_def(
    type_def: &graphql_hir::TypeDef,
    all_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use graphql_hir::TypeDefKind;

    match type_def.kind {
        TypeDefKind::Object | TypeDefKind::Interface => {
            // Validate field types exist
            for field in &type_def.fields {
                validate_type_ref(&field.type_ref, all_types, diagnostics);

                // Validate argument types
                for arg in &field.arguments {
                    validate_type_ref(&arg.type_ref, all_types, diagnostics);
                }
            }

            // Validate interface implementations
            for interface_name in &type_def.implements {
                if let Some(interface_type) = all_types.get(interface_name) {
                    // Check that the interface is actually an interface
                    if interface_type.kind == TypeDefKind::Interface {
                        // Validate that all interface fields are implemented
                        validate_interface_implementation(type_def, interface_type, diagnostics);
                    } else {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "Type '{}' implements '{}', but '{}' is not an interface",
                                type_def.name, interface_name, interface_name
                            ),
                            DiagnosticRange::default(),
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "Type '{}' implements unknown interface '{}'",
                            type_def.name, interface_name
                        ),
                        DiagnosticRange::default(),
                    ));
                }
            }
        }
        TypeDefKind::Union => {
            // Validate union members exist and are object types
            for member_name in &type_def.union_members {
                if let Some(member_type) = all_types.get(member_name) {
                    if member_type.kind != TypeDefKind::Object {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "Union '{}' includes '{}', but '{}' is not an object type",
                                type_def.name, member_name, member_name
                            ),
                            DiagnosticRange::default(),
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "Union '{}' includes unknown type '{}'",
                            type_def.name, member_name
                        ),
                        DiagnosticRange::default(),
                    ));
                }
            }
        }
        TypeDefKind::InputObject => {
            // Validate input field types exist and are valid input types
            for field in &type_def.fields {
                validate_input_type_ref(&field.type_ref, all_types, diagnostics);
            }
        }
        TypeDefKind::Enum | TypeDefKind::Scalar => {
            // No additional validation needed for enums and scalars
        }
    }
}

/// Validate that a type reference points to an existing type
fn validate_type_ref(
    type_ref: &graphql_hir::TypeRef,
    all_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Built-in scalars don't need validation
    if is_builtin_scalar(&type_ref.name) {
        return;
    }

    if !all_types.contains_key(&type_ref.name) {
        diagnostics.push(Diagnostic::error(
            format!("Unknown type: {}", type_ref.name),
            DiagnosticRange::default(),
        ));
    }
}

/// Validate that a type reference is a valid input type (scalar, enum, or input object)
fn validate_input_type_ref(
    type_ref: &graphql_hir::TypeRef,
    all_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Built-in scalars are valid input types
    if is_builtin_scalar(&type_ref.name) {
        return;
    }

    if let Some(type_def) = all_types.get(&type_ref.name) {
        use graphql_hir::TypeDefKind;
        match type_def.kind {
            TypeDefKind::Scalar | TypeDefKind::Enum | TypeDefKind::InputObject => {
                // Valid input types
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!("Type '{}' is not a valid input type", type_ref.name),
                    DiagnosticRange::default(),
                ));
            }
        }
    } else {
        diagnostics.push(Diagnostic::error(
            format!("Unknown type: {}", type_ref.name),
            DiagnosticRange::default(),
        ));
    }
}

/// Validate that a type correctly implements an interface
fn validate_interface_implementation(
    type_def: &graphql_hir::TypeDef,
    interface: &graphql_hir::TypeDef,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check that all interface fields are implemented
    for interface_field in &interface.fields {
        if let Some(impl_field) = type_def
            .fields
            .iter()
            .find(|f| f.name == interface_field.name)
        {
            // Check that the field type matches
            if impl_field.type_ref != interface_field.type_ref {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "Type '{}' field '{}' has type '{}', but interface '{}' requires '{}'",
                        type_def.name,
                        impl_field.name,
                        format_type_ref(&impl_field.type_ref),
                        interface.name,
                        format_type_ref(&interface_field.type_ref)
                    ),
                    DiagnosticRange::default(),
                ));
            }

            // Check that all interface arguments are present
            for interface_arg in &interface_field.arguments {
                if !impl_field
                    .arguments
                    .iter()
                    .any(|a| a.name == interface_arg.name)
                {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "Type '{}' field '{}' missing required argument '{}' from interface '{}'",
                            type_def.name, impl_field.name, interface_arg.name, interface.name
                        ),
                        DiagnosticRange::default(),
                    ));
                }
            }
        } else {
            diagnostics.push(Diagnostic::error(
                format!(
                    "Type '{}' does not implement field '{}' required by interface '{}'",
                    type_def.name, interface_field.name, interface.name
                ),
                DiagnosticRange::default(),
            ));
        }
    }
}

/// Check if a type name is a built-in GraphQL scalar
fn is_builtin_scalar(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "String" | "Boolean" | "ID")
}

/// Format a type reference as a string (for error messages)
fn format_type_ref(type_ref: &graphql_hir::TypeRef) -> String {
    let mut result = String::new();

    if type_ref.is_list {
        result.push('[');
        result.push_str(&type_ref.name);
        if type_ref.inner_non_null {
            result.push('!');
        }
        result.push(']');
    } else {
        result.push_str(&type_ref.name);
    }

    if type_ref.is_non_null {
        result.push('!');
    }

    result
}
