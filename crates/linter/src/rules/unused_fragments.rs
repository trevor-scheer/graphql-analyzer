use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_apollo_ext::{DocumentExt, NameExt, RangeExt};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Trait implementation for `unused_fragments` rule
pub struct UnusedFragmentsRuleImpl;

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
    /// Line offset for embedded GraphQL blocks (0 for pure GraphQL files)
    line_offset: usize,
    /// Byte offset for embedded GraphQL blocks (0 for pure GraphQL files)
    byte_offset: usize,
    /// Source for embedded GraphQL blocks (None for pure GraphQL files)
    block_source: Option<std::sync::Arc<str>>,
}

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
                for frag in doc.tree.fragments() {
                    let Some(name) = frag.name_text() else {
                        continue;
                    };
                    let Some(name_range) = frag.name_range() else {
                        continue;
                    };
                    let def_range = frag.byte_range();

                    all_fragments.push(FragmentInfo {
                        name,
                        file_id: *file_id,
                        name_start: name_range.start,
                        name_end: name_range.end,
                        def_start: def_range.start,
                        def_end: def_range.end,
                        line_offset: doc.line_offset,
                        byte_offset: doc.byte_offset,
                        block_source: if doc.byte_offset > 0 {
                            Some(std::sync::Arc::from(doc.source))
                        } else {
                            None
                        },
                    });
                }
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
        for frag_info in all_fragments {
            if !used_fragments.contains(&frag_info.name) {
                let message = format!(
                    "Fragment '{}' is defined but never used in any operation",
                    frag_info.name
                );

                let fix = CodeFix::new(
                    format!("Remove unused fragment '{}'", frag_info.name),
                    vec![TextEdit::delete(frag_info.def_start, frag_info.def_end)],
                );

                let mut diag = LintDiagnostic::warning(
                    frag_info.name_start,
                    frag_info.name_end,
                    message,
                    "unused_fragments",
                )
                .with_fix(fix);

                // Add block context for embedded GraphQL
                if let Some(block_source) = frag_info.block_source {
                    diag = diag.with_block_context(
                        frag_info.line_offset,
                        frag_info.byte_offset,
                        block_source,
                    );
                }

                diagnostics_by_file
                    .entry(frag_info.file_id)
                    .or_default()
                    .push(diag);
            }
        }

        diagnostics_by_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{
        DocumentFileIds, FileContent, FileEntry, FileEntryMap, FileId, FileKind, FileMetadata,
        FileUri, ProjectFiles, SchemaFileIds,
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
    fn test_unused_fragment_has_fix() {
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        let source = "fragment UnusedFields on User { name }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        let file_diags = diagnostics
            .get(&file_id)
            .expect("Expected diagnostics for file");
        assert_eq!(file_diags.len(), 1);

        let diag = &file_diags[0];
        assert!(diag.message.contains("UnusedFields"));
        assert!(diag.message.contains("never used"));

        // Verify fix is provided
        assert!(diag.has_fix());
        let fix = diag.fix.as_ref().unwrap();
        assert!(fix.label.contains("Remove unused fragment"));
        assert!(fix.label.contains("UnusedFields"));
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].new_text, "");

        // Verify fix range covers the entire fragment definition
        assert_eq!(fix.edits[0].offset_range.start, 0);
        assert_eq!(fix.edits[0].offset_range.end, source.len());
    }

    #[test]
    fn test_used_fragment_no_diagnostic() {
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        let source =
            "fragment UserFields on User { name } query GetUser { user { ...UserFields } }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        // No diagnostics - fragment is used
        assert!(diagnostics.is_empty() || diagnostics.get(&file_id).is_none_or(Vec::is_empty));
    }

    #[test]
    fn test_fragment_used_in_another_file() {
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        // File 1: Fragment definition
        let source1 = "fragment UserFields on User { name }";
        let file_id1 = FileId::new(0);
        let content1 = FileContent::new(&db, Arc::from(source1));
        let metadata1 = FileMetadata::new(
            &db,
            file_id1,
            FileUri::new("file:///fragments.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // File 2: Operation using the fragment
        let source2 = "query GetUser { user { ...UserFields } }";
        let file_id2 = FileId::new(1);
        let content2 = FileContent::new(&db, Arc::from(source2));
        let metadata2 = FileMetadata::new(
            &db,
            file_id2,
            FileUri::new("file:///queries.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_test_project_files(
            &db,
            &[
                (file_id1, content1, metadata1),
                (file_id2, content2, metadata2),
            ],
        );
        let diagnostics = rule.check(&db, project_files, None);

        // No diagnostics - fragment is used in another file
        assert!(
            diagnostics.is_empty()
                || (diagnostics.get(&file_id1).is_none_or(Vec::is_empty)
                    && diagnostics.get(&file_id2).is_none_or(Vec::is_empty))
        );
    }

    #[test]
    fn test_multiple_unused_fragments() {
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        let source = "fragment A on User { name } fragment B on User { email }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        let file_diags = diagnostics
            .get(&file_id)
            .expect("Expected diagnostics for file");
        assert_eq!(file_diags.len(), 2);

        // Both should have fixes
        for diag in file_diags {
            assert!(diag.has_fix());
            let fix = diag.fix.as_ref().unwrap();
            assert!(fix.label.contains("Remove unused fragment"));
            assert_eq!(fix.edits.len(), 1);
            assert_eq!(fix.edits[0].new_text, "");
        }
    }

    #[test]
    fn test_fix_range_is_accurate() {
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        // Fragment starts after the query - "query Q { user }" has a valid selection
        let source = "query Q { user } fragment Unused on User { name email }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        let file_diags = diagnostics
            .get(&file_id)
            .expect("Expected diagnostics for file");
        assert_eq!(file_diags.len(), 1);

        let diag = &file_diags[0];
        let fix = diag.fix.as_ref().unwrap();

        // Verify the fix deletes exactly the fragment definition
        let deleted_text = &source[fix.edits[0].offset_range.start..fix.edits[0].offset_range.end];
        assert!(deleted_text.starts_with("fragment Unused"));
        assert!(deleted_text.ends_with('}'));
    }

    #[test]
    fn test_diagnostic_range_points_to_fragment_name() {
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        let source = "fragment UnusedFragment on User { name }";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        let file_diags = diagnostics
            .get(&file_id)
            .expect("Expected diagnostics for file");
        assert_eq!(file_diags.len(), 1);

        let diag = &file_diags[0];
        // Diagnostic range should point to the fragment name "UnusedFragment"
        let name_text = &source[diag.offset_range.start..diag.offset_range.end];
        assert_eq!(name_text, "UnusedFragment");
    }
}
