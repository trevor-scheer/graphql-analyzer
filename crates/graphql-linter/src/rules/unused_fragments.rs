use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, ProjectLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Trait implementation for `unused_fragments` rule
pub struct UnusedFragmentsRuleImpl;

impl LintRule for UnusedFragmentsRuleImpl {
    fn name(&self) -> &'static str {
        "unused_fragments"
    }

    fn description(&self) -> &'static str {
        "Detects fragment definitions that are never used in any operation"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// Information about a fragment definition for fix computation
struct FragmentInfo {
    /// Fragment name
    name: String,
    /// File where the fragment is defined
    file_id: FileId,
    /// Byte offset of the fragment name (for diagnostic range)
    name_start: usize,
    /// Byte offset of the end of the fragment name
    name_end: usize,
    /// Byte offset of the entire fragment definition
    def_start: usize,
    /// Byte offset of the end of the fragment definition
    def_end: usize,
}

impl ProjectLintRule for UnusedFragmentsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Step 1: Collect all fragment definitions with their CST positions
        let doc_ids = project_files.document_file_ids(db).ids(db);
        let mut all_fragments: Vec<FragmentInfo> = Vec::new();

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            // Parse the file to get CST positions
            let parse = graphql_syntax::parse(db, content, metadata);
            if parse.has_errors() {
                continue;
            }

            // Iterate over all GraphQL documents (unified API for .graphql and TS/JS)
            for doc in parse.documents() {
                collect_fragment_definitions(&doc.tree.document(), *file_id, &mut all_fragments);
            }
        }

        // Step 2: Collect all used fragment names using per-file cached queries
        let mut used_fragments: HashSet<String> = HashSet::new();

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            let used = graphql_hir::file_used_fragment_names(db, *file_id, content, metadata);
            for fragment_name in used.iter() {
                used_fragments.insert(fragment_name.to_string());
            }
        }

        // Step 3: Report unused fragments with fixes
        for frag_info in &all_fragments {
            if !used_fragments.contains(&frag_info.name) {
                let message = format!(
                    "Fragment '{}' is defined but never used in any operation",
                    frag_info.name
                );

                let fix = CodeFix::new(
                    format!("Remove unused fragment '{}'", frag_info.name),
                    vec![TextEdit::delete(frag_info.def_start, frag_info.def_end)],
                );

                let diag = LintDiagnostic::warning(
                    frag_info.name_start,
                    frag_info.name_end,
                    message,
                    "unused_fragments",
                )
                .with_fix(fix);

                diagnostics_by_file
                    .entry(frag_info.file_id)
                    .or_default()
                    .push(diag);
            }
        }

        diagnostics_by_file
    }
}

/// Collect fragment definitions from a CST document with their positions
fn collect_fragment_definitions(
    doc: &cst::Document,
    file_id: FileId,
    fragments: &mut Vec<FragmentInfo>,
) {
    for definition in doc.definitions() {
        if let cst::Definition::FragmentDefinition(frag) = definition {
            let Some(fragment_name) = frag.fragment_name() else {
                continue;
            };
            let Some(name) = fragment_name.name() else {
                continue;
            };

            let name_syntax = name.syntax();
            let name_start: usize = name_syntax.text_range().start().into();
            let name_end: usize = name_syntax.text_range().end().into();

            let def_syntax = frag.syntax();
            let def_start: usize = def_syntax.text_range().start().into();
            let def_end: usize = def_syntax.text_range().end().into();

            fragments.push(FragmentInfo {
                name: name.text().to_string(),
                file_id,
                name_start,
                name_end,
                def_start,
                def_end,
            });
        }
    }
}
