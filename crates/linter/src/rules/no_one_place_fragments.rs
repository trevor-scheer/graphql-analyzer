use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_apollo_ext::{DocumentExt, NameExt};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Lint rule that detects fragments used in only one place
///
/// Fragments used in only one location should be inlined to reduce complexity.
/// If a fragment is only spread once, it adds indirection without the benefit
/// of reuse.
pub struct NoOnePlaceFragmentsRuleImpl;

impl LintRule for NoOnePlaceFragmentsRuleImpl {
    fn name(&self) -> &'static str {
        "no_one_place_fragments"
    }

    fn description(&self) -> &'static str {
        "Detects fragments that are used in only one place and could be inlined"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl ProjectLintRule for NoOnePlaceFragmentsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let doc_ids = project_files.document_file_ids(db).ids(db);

        // Step 1: Count how many times each fragment is spread
        let mut fragment_usage_count: HashMap<String, usize> = HashMap::new();

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            let used = graphql_hir::file_used_fragment_names(db, *file_id, content, metadata);
            for fragment_name in used.iter() {
                *fragment_usage_count
                    .entry(fragment_name.to_string())
                    .or_insert(0) += 1;
            }
        }

        // Step 2: Find one-place fragments by collecting all fragment definitions and
        // counting unique usage sites
        let mut fragment_spread_sites: HashMap<String, HashSet<(FileId, String)>> = HashMap::new();

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            let parse = graphql_syntax::parse(db, content, metadata);
            if parse.has_errors() {
                continue;
            }

            for doc in parse.documents() {
                // Track which definition uses which fragments
                for def in doc.tree.document().definitions() {
                    let def_name = match &def {
                        apollo_parser::cst::Definition::OperationDefinition(op) => {
                            op.name().map(|n| n.text().to_string()).unwrap_or_default()
                        }
                        apollo_parser::cst::Definition::FragmentDefinition(frag) => frag
                            .fragment_name()
                            .and_then(|fn_| fn_.name())
                            .map(|n| n.text().to_string())
                            .unwrap_or_default(),
                        _ => continue,
                    };

                    collect_fragment_spreads_from_definition(
                        &def,
                        *file_id,
                        &def_name,
                        &mut fragment_spread_sites,
                    );
                }
            }
        }

        // Step 3: Find fragment definitions that have exactly one spread site
        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            let parse = graphql_syntax::parse(db, content, metadata);
            if parse.has_errors() {
                continue;
            }

            for doc in parse.documents() {
                for frag in doc.tree.fragments() {
                    let Some(name) = frag.name_text() else {
                        continue;
                    };

                    if let Some(sites) = fragment_spread_sites.get(&name) {
                        if sites.len() == 1 {
                            let Some(name_range) = frag.name_range() else {
                                continue;
                            };

                            diagnostics_by_file.entry(*file_id).or_default().push(
                                LintDiagnostic::warning(
                                    doc.span(name_range.start, name_range.end),
                                    format!(
                                        "Fragment '{name}' is used in only one place. Consider inlining it."
                                    ),
                                    "no_one_place_fragments",
                                ),
                            );
                        }
                    }
                }
            }
        }

        diagnostics_by_file
    }
}

fn collect_fragment_spreads_from_definition(
    def: &apollo_parser::cst::Definition,
    file_id: FileId,
    def_name: &str,
    fragment_spread_sites: &mut HashMap<String, HashSet<(FileId, String)>>,
) {
    use apollo_parser::cst;

    let selection_set = match def {
        cst::Definition::OperationDefinition(op) => op.selection_set(),
        cst::Definition::FragmentDefinition(frag) => frag.selection_set(),
        _ => return,
    };

    if let Some(selection_set) = selection_set {
        collect_spreads_in_selection_set(&selection_set, file_id, def_name, fragment_spread_sites);
    }
}

fn collect_spreads_in_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
    file_id: FileId,
    def_name: &str,
    fragment_spread_sites: &mut HashMap<String, HashSet<(FileId, String)>>,
) {
    use apollo_parser::cst;

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(nested) = field.selection_set() {
                    collect_spreads_in_selection_set(
                        &nested,
                        file_id,
                        def_name,
                        fragment_spread_sites,
                    );
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(name) = spread.fragment_name().and_then(|fn_| fn_.name()) {
                    let frag_name = name.text().to_string();
                    fragment_spread_sites
                        .entry(frag_name)
                        .or_default()
                        .insert((file_id, def_name.to_string()));
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    collect_spreads_in_selection_set(
                        &nested,
                        file_id,
                        def_name,
                        fragment_spread_sites,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, ProjectFiles, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(
        db: &RootDatabase,
        doc_files: &[(FileId, FileContent, FileMetadata)],
    ) -> ProjectFiles {
        let mut entries = std::collections::HashMap::new();
        for (file_id, content, metadata) in doc_files {
            let entry = FileEntry::new(db, *content, *metadata);
            entries.insert(*file_id, entry);
        }
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = DocumentFileIds::new(
            db,
            Arc::new(doc_files.iter().map(|(id, _, _)| *id).collect()),
        );
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_fragment_used_once() {
        let db = RootDatabase::default();
        let rule = NoOnePlaceFragmentsRuleImpl;

        let source = "fragment F on User { name } query Q { user { ...F } }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        let file_diags = diagnostics.get(&file_id);
        assert!(
            file_diags.is_some_and(|d| d.len() == 1),
            "Expected one diagnostic for one-place fragment"
        );
    }

    #[test]
    fn test_fragment_used_multiple_times() {
        let db = RootDatabase::default();
        let rule = NoOnePlaceFragmentsRuleImpl;

        let source =
            "fragment F on User { name } query Q1 { user { ...F } } query Q2 { users { ...F } }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        let file_diags = diagnostics.get(&file_id);
        assert!(
            file_diags.is_none() || file_diags.is_some_and(Vec::is_empty),
            "Expected no diagnostic for fragment used in multiple places"
        );
    }
}
