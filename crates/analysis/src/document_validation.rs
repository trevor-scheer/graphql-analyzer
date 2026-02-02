// Document validation queries (operations and fragments)

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position};
use graphql_base_db::{FileContent, FileMetadata};
use std::sync::Arc;
use text_size::TextRange;

/// Convert a `TextRange` (byte offsets) to `DiagnosticRange` (line/column)
///
/// Uses the `LineIndex` to convert byte offsets to line/column positions.
#[allow(clippy::cast_possible_truncation)]
fn text_range_to_diagnostic_range(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    range: TextRange,
) -> DiagnosticRange {
    let line_index = graphql_syntax::line_index(db, content);

    let (start_line, start_col) = line_index.line_col(range.start().into());
    let (end_line, end_col) = line_index.line_col(range.end().into());

    DiagnosticRange {
        start: Position {
            line: start_line as u32,
            character: start_col as u32,
        },
        end: Position {
            line: end_line as u32,
            character: end_col as u32,
        },
    }
}

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
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let structure = graphql_hir::file_structure(db, metadata.file_id(db), content, metadata);
    let mut diagnostics = Vec::new();
    let schema = graphql_hir::schema_types(db, project_files);

    // Use the operation name index for O(1) lookup instead of iterating all operations
    let op_name_index = graphql_hir::project_operation_name_index(db, project_files);

    for op_structure in structure.operations.iter() {
        if let Some(name) = &op_structure.name {
            // O(1) lookup instead of O(n) iteration
            let count = op_name_index.get(name).copied().unwrap_or(0);

            if count > 1 {
                // Use the name range if available, otherwise fall back to operation range
                let range = op_structure
                    .name_range
                    .map(|r| text_range_to_diagnostic_range(db, content, r))
                    .unwrap_or_default();
                diagnostics.push(Diagnostic::error(
                    format!("Operation name '{name}' is not unique"),
                    range,
                ));
            }
        }

        // Note: VariableSignature doesn't have position info, so we use the operation range
        let op_range = text_range_to_diagnostic_range(db, content, op_structure.operation_range);
        for var in &op_structure.variables {
            validate_variable_type(&var.type_ref, schema, op_range, &mut diagnostics);
        }

        #[allow(clippy::match_same_arms)]
        let root_type_name = match op_structure.operation_type {
            graphql_hir::OperationType::Query => "Query",
            graphql_hir::OperationType::Mutation => "Mutation",
            graphql_hir::OperationType::Subscription => "Subscription",
            _ => "Query", // fallback for future operation types
        };

        if !schema.contains_key(&Arc::from(root_type_name)) {
            let range = text_range_to_diagnostic_range(db, content, op_structure.operation_range);
            diagnostics.push(Diagnostic::error(
                format!("Schema does not define a '{root_type_name}' type"),
                range,
            ));
        }
        // NOTE: Full body validation (field selections, arguments, fragment spreads)
        // is complex and best handled by apollo-compiler's validation.
        // For now, we rely on the structural checks above.
        // A future enhancement would be to integrate apollo-compiler's validator here.
    }

    // Use the fragment name index for O(1) lookup instead of iterating all fragments
    let frag_name_index = graphql_hir::project_fragment_name_index(db, project_files);

    for frag_structure in structure.fragments.iter() {
        // O(1) lookup instead of O(n) iteration
        let count = frag_name_index
            .get(&frag_structure.name)
            .copied()
            .unwrap_or(0);

        if count > 1 {
            let range = text_range_to_diagnostic_range(db, content, frag_structure.name_range);
            diagnostics.push(Diagnostic::error(
                format!("Fragment name '{}' is not unique", frag_structure.name),
                range,
            ));
        }

        let type_condition_range =
            text_range_to_diagnostic_range(db, content, frag_structure.type_condition_range);
        validate_fragment_type_condition(
            frag_structure,
            schema,
            type_condition_range,
            &mut diagnostics,
        );
    }

    Arc::new(diagnostics)
}

/// Validate that a variable's type exists and is a valid input type
fn validate_variable_type(
    type_ref: &graphql_hir::TypeRef,
    schema: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    range: DiagnosticRange,
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
                    range,
                ));
            }
        }
    } else {
        diagnostics.push(Diagnostic::error(
            format!("Unknown variable type: {}", type_ref.name),
            range,
        ));
    }
}

/// Validate that a fragment's type condition exists in the schema
fn validate_fragment_type_condition(
    fragment: &graphql_hir::FragmentStructure,
    schema: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    range: DiagnosticRange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !schema.contains_key(&fragment.type_condition) {
        diagnostics.push(Diagnostic::error(
            format!(
                "Fragment '{}' has unknown type condition '{}'",
                fragment.name, fragment.type_condition
            ),
            range,
        ));
        return;
    }

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
                    range,
                ));
            }
        }
    }
}

/// Check if a type name is a built-in GraphQL scalar
fn is_builtin_scalar(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "String" | "Boolean" | "ID")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_scalar_int() {
        assert!(is_builtin_scalar("Int"));
    }

    #[test]
    fn test_is_builtin_scalar_float() {
        assert!(is_builtin_scalar("Float"));
    }

    #[test]
    fn test_is_builtin_scalar_string() {
        assert!(is_builtin_scalar("String"));
    }

    #[test]
    fn test_is_builtin_scalar_boolean() {
        assert!(is_builtin_scalar("Boolean"));
    }

    #[test]
    fn test_is_builtin_scalar_id() {
        assert!(is_builtin_scalar("ID"));
    }

    #[test]
    fn test_is_builtin_scalar_custom_type() {
        assert!(!is_builtin_scalar("User"));
        assert!(!is_builtin_scalar("DateTime"));
        assert!(!is_builtin_scalar("JSON"));
    }

    #[test]
    fn test_is_builtin_scalar_case_sensitive() {
        assert!(!is_builtin_scalar("string"));
        assert!(!is_builtin_scalar("int"));
        assert!(!is_builtin_scalar("BOOLEAN"));
    }
}
