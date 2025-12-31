use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_hir::{FieldId, FragmentId};
use std::collections::{HashMap, HashSet, VecDeque};
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
#[salsa::tracked]
pub fn find_unused_fragments(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<Vec<(FragmentId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);
    let doc_ids = project_files.document_file_ids(db).ids(db);

    let mut used_fragments = HashSet::new();

    // First, collect all ASTs for cross-file fragment resolution
    let mut all_documents = Vec::new();
    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        let Some((file_content, file_metadata)) =
            graphql_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };
        let parse = graphql_syntax::parse(db, file_content, file_metadata);

        // Collect ASTs from all documents (works for both pure GraphQL and TS/JS)
        for doc in parse.documents() {
            all_documents.push(Arc::new(doc.ast.clone()));
        }
    }

    // Collect fragment spreads from all documents
    for document in &all_documents {
        collect_fragment_spreads_recursive(
            document,
            &all_documents,
            &all_fragments,
            &mut used_fragments,
        );
    }

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

/// Collect fragment spreads from an AST document recursively (including transitive dependencies)
fn collect_fragment_spreads_recursive(
    document: &apollo_compiler::ast::Document,
    all_documents: &[Arc<apollo_compiler::ast::Document>],
    all_fragments: &HashMap<Arc<str>, graphql_hir::FragmentStructure>,
    used_fragments: &mut HashSet<Arc<str>>,
) {
    use apollo_compiler::ast::Definition;

    // Collect direct fragment spreads from operations
    let mut to_process: VecDeque<Arc<str>> = VecDeque::new();

    for definition in &document.definitions {
        if let Definition::OperationDefinition(op) = definition {
            collect_fragment_spreads_from_selection_set(&op.selection_set, &mut to_process);
        }
        // FragmentDefinitions and other definition types don't need processing here
        // Fragments will be processed when referenced by operations
    }

    // Process fragments transitively
    while let Some(fragment_name) = to_process.pop_front() {
        // Skip if already processed
        if used_fragments.contains(&fragment_name) {
            continue;
        }

        used_fragments.insert(fragment_name.clone());

        // Find the fragment definition across all documents
        if all_fragments.contains_key(&fragment_name) {
            // Look for the fragment definition in all documents
            for doc in all_documents {
                for definition in &doc.definitions {
                    if let Definition::FragmentDefinition(frag) = definition {
                        if frag.name.as_str() == fragment_name.as_ref() {
                            collect_fragment_spreads_from_selection_set(
                                &frag.selection_set,
                                &mut to_process,
                            );
                            break; // Found the fragment, no need to check more definitions
                        }
                    }
                }
            }
        }
    }
}

/// Collect fragment spreads from a selection set (`ast::Selection`)
fn collect_fragment_spreads_from_selection_set(
    selections: &[apollo_compiler::ast::Selection],
    fragment_names: &mut VecDeque<Arc<str>>,
) {
    use apollo_compiler::ast::Selection;

    for selection in selections {
        match selection {
            Selection::Field(field) => {
                // Recurse into nested selection sets
                collect_fragment_spreads_from_selection_set(&field.selection_set, fragment_names);
            }
            Selection::FragmentSpread(spread) => {
                // Add fragment name to the list
                fragment_names.push_back(Arc::from(spread.fragment_name.as_str()));
            }
            Selection::InlineFragment(inline) => {
                // Recurse into inline fragment selection set
                collect_fragment_spreads_from_selection_set(&inline.selection_set, fragment_names);
            }
        }
    }
}
