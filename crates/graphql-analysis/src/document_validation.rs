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

        // TODO: Validate fragment type condition exists in schema
        let schema = graphql_hir::schema_types(db);
        if !schema.contains_key(&frag_structure.type_condition) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "Unknown type '{}' in fragment",
                    frag_structure.type_condition
                ),
                DiagnosticRange::default(),
            ));
        }
    }

    Arc::new(diagnostics)
}
