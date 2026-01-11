use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_hir::{FieldId, FragmentId};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

// TODO(trevor): implement these queries
#[salsa::tracked]
pub fn find_unused_fields(db: &dyn GraphQLAnalysisDatabase) -> Arc<Vec<(FieldId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let _schema = graphql_hir::schema_types_with_project(db, project_files);
    let _operations = graphql_hir::all_operations(db, project_files);

    let unused = Vec::new();

    // TODO(trevor): Implement unused field detection
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
///
/// Uses HIR queries for fragment data instead of cloning ASTs.
/// This avoids massive memory allocation when processing large projects.
#[salsa::tracked]
pub fn find_unused_fragments(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<Vec<(FragmentId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);

    // Use the fragment spreads index from HIR (cached, no AST cloning needed)
    let fragment_spreads_index = graphql_hir::fragment_spreads_index(db, project_files);

    let mut used_fragments = HashSet::new();

    // Collect fragment spreads from operations (using HIR data, not ASTs)
    let doc_ids = project_files.document_file_ids(db).ids(db);
    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) else {
            continue;
        };

        // Get operation bodies from cached HIR queries
        let file_ops = graphql_hir::file_operations(db, *file_id, content, metadata);
        for (op_index, _op) in file_ops.iter().enumerate() {
            let body = graphql_hir::operation_body(db, content, metadata, op_index);
            // Add direct fragment spreads from this operation
            for spread in &body.fragment_spreads {
                collect_fragment_transitive(spread, &fragment_spreads_index, &mut used_fragments);
            }
        }
    }

    // Fragment spreads from fragment-to-fragment references are already handled
    // by the transitive collection above. The fragment_spreads_index contains
    // the direct spreads for each fragment, and collect_fragment_transitive
    // follows them recursively.

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
                    DiagnosticRange::default(), // Position would require CST traversal
                ),
            ));
        }
    }

    Arc::new(unused)
}

/// Collect a fragment and all fragments it transitively spreads
fn collect_fragment_transitive(
    fragment_name: &Arc<str>,
    fragment_spreads_index: &std::collections::HashMap<Arc<str>, HashSet<Arc<str>>>,
    used_fragments: &mut HashSet<Arc<str>>,
) {
    let mut to_process: VecDeque<Arc<str>> = VecDeque::new();
    to_process.push_back(fragment_name.clone());

    while let Some(name) = to_process.pop_front() {
        if used_fragments.contains(&name) {
            continue;
        }
        used_fragments.insert(name.clone());

        // Add transitive dependencies from the index
        if let Some(spreads) = fragment_spreads_index.get(&name) {
            for spread in spreads {
                if !used_fragments.contains(spread) {
                    to_process.push_back(spread.clone());
                }
            }
        }
    }
}
