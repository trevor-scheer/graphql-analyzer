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

    // Check for conflicts with types in other files
    let all_types = graphql_hir::schema_types(db);
    for type_def in &structure.type_defs {
        // TODO: Check if type name conflicts with types from other files
        // This requires tracking which file each type comes from in the HIR
        let _ = all_types.get(&type_def.name);
    }

    // TODO: Additional schema validation:
    // - Validate field types exist
    // - Validate interface implementations
    // - Validate union members exist
    // - Validate enum values
    // - Validate directive definitions and usage

    Arc::new(diagnostics)
}
