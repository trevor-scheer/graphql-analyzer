// Project-wide lint queries

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_hir::{FieldId, FragmentId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Find unused fields (project-wide analysis)
/// This is expensive and should only run when explicitly requested
#[salsa::tracked]
pub fn find_unused_fields(db: &dyn GraphQLAnalysisDatabase) -> Arc<Vec<(FieldId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let _schema = graphql_hir::schema_types_with_project(db, project_files);
    let _operations = graphql_hir::all_operations(db);

    let unused = Vec::new();

    // TODO: Implement unused field detection
    // This requires:
    // 1. Parsing operation bodies to extract field selections
    // 2. Following fragment spreads (transitive)
    // 3. Collecting all used fields across all operations
    // 4. Comparing with schema fields to find unused ones
    //
    // This is complex and requires body parsing integration.
    // For now, we leave this as a stub.

    Arc::new(unused)
}

/// Find unused fragments (project-wide analysis)
#[salsa::tracked]
pub fn find_unused_fragments(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<Vec<(FragmentId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);
    let all_operations = graphql_hir::all_operations(db);

    // Collect all used fragments by walking operations
    let mut used_fragments = HashSet::new();

    // Get the parse results for all operations to extract fragment spreads
    // We need to parse the operation bodies
    for operation in all_operations.iter() {
        // NOTE: To properly implement this, we need access to the parsed AST
        // from the operation's file. This requires either:
        // 1. Adding a query to get the parsed document for a file
        // 2. Storing the AST in the operation structure
        // 3. Re-parsing the file here (inefficient)
        //
        // For now, we'll implement a basic version that requires integration
        // with the syntax layer to get fragment spreads.

        // Collect fragment spreads from this operation (transitively)
        collect_fragment_spreads_from_operation(db, operation, &all_fragments, &mut used_fragments);
    }

    // Find unused fragments
    let mut unused = Vec::new();
    for (fragment_name, _fragment_structure) in all_fragments.iter() {
        if !used_fragments.contains(fragment_name) {
            // Create a dummy FragmentId - in a real implementation,
            // we'd track the actual FragmentId in the HIR
            let fragment_id = FragmentId::new(unsafe { salsa::Id::from_index(0) });

            unused.push((
                fragment_id,
                Diagnostic::warning(
                    format!("Fragment '{fragment_name}' is never used"),
                    DiagnosticRange::default(), // TODO: Get actual position
                ),
            ));
        }
    }

    Arc::new(unused)
}

/// Collect fragment spreads from an operation (transitively)
/// This is a placeholder - proper implementation requires body parsing
fn collect_fragment_spreads_from_operation(
    db: &dyn GraphQLAnalysisDatabase,
    operation: &graphql_hir::OperationStructure,
    all_fragments: &HashMap<Arc<str>, graphql_hir::FragmentStructure>,
    used_fragments: &mut HashSet<Arc<str>>,
) {
    // TODO: Implement fragment spread collection
    // This requires:
    // 1. Getting the parsed AST for the operation's file
    // 2. Walking the operation's selection set
    // 3. Collecting fragment spreads
    // 4. Recursively following fragment spreads to get transitive dependencies
    //
    // For now, this is a stub that doesn't mark any fragments as used.
    // This means all fragments will be reported as unused, which is incorrect
    // but demonstrates the diagnostic infrastructure.

    let _ = (db, operation, all_fragments, used_fragments);
}
