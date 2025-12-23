// Project-wide lint queries

use crate::{Diagnostic, GraphQLAnalysisDatabase};
use graphql_hir::{FieldId, FragmentId};
use std::sync::Arc;

/// Find unused fields (project-wide analysis)
/// This is expensive and should only run when explicitly requested
#[salsa::tracked]
pub fn find_unused_fields(db: &dyn GraphQLAnalysisDatabase) -> Arc<Vec<(FieldId, Diagnostic)>> {
    let _schema = graphql_hir::schema_types(db);
    let _operations = graphql_hir::all_operations(db);

    let unused = Vec::new();

    // TODO: Implement unused field detection
    // 1. Collect all fields used in operations
    //    - Walk operation bodies
    //    - Follow fragment spreads (transitive)
    // 2. Compare with all fields in schema
    // 3. Report fields that are never used

    Arc::new(unused)
}

/// Find unused fragments (project-wide analysis)
#[salsa::tracked]
pub fn find_unused_fragments(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<Vec<(FragmentId, Diagnostic)>> {
    let _all_fragments = graphql_hir::all_fragments(db);
    let _all_operations = graphql_hir::all_operations(db);

    let unused = Vec::new();

    // TODO: Implement unused fragment detection
    // 1. For each operation, collect all fragment dependencies (transitive)
    // 2. Any fragment not in that set is unused

    Arc::new(unused)
}
