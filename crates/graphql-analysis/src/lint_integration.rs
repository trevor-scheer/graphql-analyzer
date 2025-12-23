// Integration with graphql-linter

use crate::{Diagnostic, GraphQLAnalysisDatabase};
use graphql_db::{FileContent, FileMetadata};
use std::sync::Arc;

/// Run lints on a file
/// This integrates with the existing graphql-linter crate
#[salsa::tracked]
pub fn lint_file(
    db: &dyn GraphQLAnalysisDatabase,
    _content: FileContent,
    _metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let _lint_config = db.lint_config();
    let diagnostics = Vec::new();

    // TODO: Integrate with graphql_linter::Linter
    // This will require:
    // 1. Building SchemaIndex and DocumentIndex from HIR
    // 2. Calling linter methods
    // 3. Converting lint diagnostics to our Diagnostic type

    Arc::new(diagnostics)
}
