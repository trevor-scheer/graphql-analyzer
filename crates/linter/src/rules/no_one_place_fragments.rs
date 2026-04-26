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
        "noOnePlaceFragments"
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

                            // Mirror graphql-eslint's message: `Inline him in "{filePath}".`,
                            // where `filePath` is the usage site's path relative to CWD.
                            let usage_file_id = sites.iter().next().map(|(id, _)| *id);
                            let usage_path = usage_file_id.and_then(|id| {
                                graphql_base_db::file_lookup(db, project_files, id)
                                    .map(|(_, meta)| meta.uri(db).as_str().to_string())
                            });
                            let message = match usage_path.as_deref().map(cwd_relative_path) {
                                Some(rel) => format!(
                                    "Fragment `{name}` used only once. Inline him in \"{rel}\"."
                                ),
                                None => format!("Fragment `{name}` used only once."),
                            };

                            diagnostics_by_file.entry(*file_id).or_default().push(
                                LintDiagnostic::warning(
                                    doc.span(name_range.start, name_range.end),
                                    message,
                                    "noOnePlaceFragments",
                                )
                                .with_message_id("no-one-place-fragments")
                                .with_help("Inline the fragment at its single usage site")
                                .with_tag(crate::diagnostics::DiagnosticTag::Unnecessary),
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

/// Convert a file URI or absolute path to a CWD-relative path, mirroring
/// graphql-eslint's `path.relative(process.cwd(), filePath)` behaviour.
///
/// Falls back to the original string when the URI can't be resolved against the
/// current working directory (e.g. virtual `file:///` paths in tests where the
/// usage site lives outside CWD, or when `current_dir()` fails).
fn cwd_relative_path(uri_or_path: &str) -> String {
    use std::path::{Component, Path, PathBuf};

    let path_str = uri_or_path.strip_prefix("file://").unwrap_or(uri_or_path);
    let target = Path::new(path_str);
    let Ok(cwd) = std::env::current_dir() else {
        return path_str.to_string();
    };

    // Only meaningful when both paths are absolute; otherwise just echo the input
    // (matches Node's behaviour of resolving relative-to-cwd, which is a no-op here).
    if !target.is_absolute() {
        return path_str.to_string();
    }

    let cwd_components: Vec<Component> = cwd.components().collect();
    let target_components: Vec<Component> = target.components().collect();
    let common = cwd_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut out = PathBuf::new();
    for _ in common..cwd_components.len() {
        out.push("..");
    }
    for comp in &target_components[common..] {
        out.push(comp.as_os_str());
    }

    if out.as_os_str().is_empty() {
        String::new()
    } else {
        out.to_string_lossy().into_owned()
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
        ProjectFiles::new(
            db,
            schema_file_ids,
            document_file_ids,
            graphql_base_db::ResolvedSchemaFileIds::new(db, std::sync::Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        )
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
        let diag = &file_diags.unwrap()[0];
        assert!(
            diag.message.starts_with("Fragment `F` used only once."),
            "unexpected message: {}",
            diag.message
        );
        assert!(
            diag.message.contains("Inline him in \""),
            "missing `Inline him in` suffix; got: {}",
            diag.message
        );
        assert!(
            diag.message.contains("test.graphql"),
            "expected usage-site path in message; got: {}",
            diag.message
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
