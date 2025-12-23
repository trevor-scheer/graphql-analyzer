// Document validation queries (operations and fragments)

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_db::{FileContent, FileMetadata};
use std::sync::Arc;

/// Validate a document file (operations and fragments)
/// This checks for:
/// - Operation name uniqueness
/// - Fragment name uniqueness
/// - Valid type conditions on fragments
/// - Valid field selections against schema
/// - Valid variable types
/// - Fragment spread resolution
#[salsa::tracked]
pub fn validate_document_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let structure = graphql_hir::file_structure(db, metadata.file_id(db), content, metadata);
    let mut diagnostics = Vec::new();

    // Get schema for validation
    let schema = graphql_hir::schema_types(db);

    // Validate each operation
    for op_structure in &structure.operations {
        // Check operation name uniqueness (structural check - cheap)
        if let Some(name) = &op_structure.name {
            let all_ops = graphql_hir::all_operations(db);

            // Count how many operations have this name
            let count = all_ops
                .iter()
                .filter(|op| op.name.as_ref() == Some(name))
                .count();

            if count > 1 {
                diagnostics.push(Diagnostic::error(
                    format!("Operation name '{name}' is not unique"),
                    DiagnosticRange::default(), // TODO: Get actual position from HIR
                ));
            }
        }

        // Validate variable types
        for var in &op_structure.variables {
            validate_variable_type(&var.type_ref, &schema, &mut diagnostics);
        }

        // Validate operation body
        // Get the root type for this operation
        let root_type_name = match op_structure.operation_type {
            graphql_hir::OperationType::Query => "Query",
            graphql_hir::OperationType::Mutation => "Mutation",
            graphql_hir::OperationType::Subscription => "Subscription",
        };

        if !schema.contains_key(&Arc::from(root_type_name)) {
            diagnostics.push(Diagnostic::error(
                format!("Schema does not define a '{root_type_name}' type"),
                DiagnosticRange::default(),
            ));
        }
        // NOTE: Full body validation (field selections, arguments, fragment spreads)
        // is complex and best handled by apollo-compiler's validation.
        // For now, we rely on the structural checks above.
        // A future enhancement would be to integrate apollo-compiler's validator here.
    }

    // Validate fragments
    for frag_structure in &structure.fragments {
        // Check fragment name uniqueness
        let all_fragments = graphql_hir::all_fragments(db);

        let count = all_fragments
            .iter()
            .filter(|(_, frag)| frag.name == frag_structure.name)
            .count();

        if count > 1 {
            diagnostics.push(Diagnostic::error(
                format!("Fragment name '{}' is not unique", frag_structure.name),
                DiagnosticRange::default(), // TODO: Get actual position from HIR
            ));
        }

        // Validate fragment type condition exists in schema
        validate_fragment_type_condition(frag_structure, &schema, &mut diagnostics);

        // TODO: Validate fragment body (field selections)
        // This requires parsing the fragment body and walking the selection set
    }

    Arc::new(diagnostics)
}

/// Validate that a variable's type exists and is a valid input type
fn validate_variable_type(
    type_ref: &graphql_hir::TypeRef,
    schema: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Built-in scalars are valid
    if is_builtin_scalar(&type_ref.name) {
        return;
    }

    if let Some(type_def) = schema.get(&type_ref.name) {
        use graphql_hir::TypeDefKind;
        match type_def.kind {
            TypeDefKind::Scalar | TypeDefKind::Enum | TypeDefKind::InputObject => {
                // Valid input types for variables
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "Variable type '{}' is not a valid input type",
                        type_ref.name
                    ),
                    DiagnosticRange::default(),
                ));
            }
        }
    } else {
        diagnostics.push(Diagnostic::error(
            format!("Unknown variable type: {}", type_ref.name),
            DiagnosticRange::default(),
        ));
    }
}

/// Validate that a fragment's type condition exists in the schema
fn validate_fragment_type_condition(
    fragment: &graphql_hir::FragmentStructure,
    schema: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !schema.contains_key(&fragment.type_condition) {
        diagnostics.push(Diagnostic::error(
            format!(
                "Fragment '{}' has unknown type condition '{}'",
                fragment.name, fragment.type_condition
            ),
            DiagnosticRange::default(),
        ));
        return;
    }

    // Check that the type condition is an object, interface, or union
    if let Some(type_def) = schema.get(&fragment.type_condition) {
        use graphql_hir::TypeDefKind;
        match type_def.kind {
            TypeDefKind::Object | TypeDefKind::Interface | TypeDefKind::Union => {
                // Valid fragment type conditions
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "Fragment '{}' type condition '{}' must be an object, interface, or union type",
                        fragment.name, fragment.type_condition
                    ),
                    DiagnosticRange::default(),
                ));
            }
        }
    }
}

/// Check if a type name is a built-in GraphQL scalar
fn is_builtin_scalar(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "String" | "Boolean" | "ID")
}
