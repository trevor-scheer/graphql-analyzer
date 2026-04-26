use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_apollo_ext::{DocumentExt, NameExt, RangeExt};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Length of the literal `fragment` keyword in bytes — graphql-eslint's
/// adapter re-anchors the diagnostic onto this token, so we span the same
/// range for parity.
const FRAGMENT_KEYWORD_LEN: usize = 8;

/// Trait implementation for `no_unused_fragments` rule
pub struct NoUnusedFragmentsRuleImpl;

/// Information about a fragment definition for fix computation
struct FragmentInfo {
    /// Fragment name
    name: String,
    /// File where the fragment is defined
    file_id: FileId,
    /// Source span for the fragment name (carries block context for TS/JS)
    name_span: graphql_syntax::SourceSpan,
    /// Byte offset of the entire fragment definition (for fix range)
    def_start: usize,
    /// Byte offset of the end of the fragment definition (for fix range)
    def_end: usize,
    /// File-level byte range of the enclosing TS/JS declaration
    declaration_range: Option<(usize, usize)>,
    /// Number of GraphQL definitions in the containing block
    block_def_count: usize,
}

impl LintRule for NoUnusedFragmentsRuleImpl {
    fn name(&self) -> &'static str {
        "noUnusedFragments"
    }

    fn description(&self) -> &'static str {
        "Detects fragment definitions that are never used in any operation"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl ProjectLintRule for NoUnusedFragmentsRuleImpl {
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
                        name_span: doc.span(name_range.start, name_range.end),
                        def_start: def_range.start,
                        def_end: def_range.end,
                        declaration_range: doc.declaration_range,
                        block_def_count: doc.ast.definitions.len(),
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
                // Mirror graphql-eslint exactly: drop-in users see the same
                // text and source positions on `LintMessage` as
                // `@graphql-eslint/eslint-plugin`. graphql-eslint's adapter
                // re-locates the diagnostic onto the `fragment` keyword token
                // (the start of `fragmentDef`), not the fragment name; the
                // message comes from graphql-js's `NoUnusedFragmentsRule`.
                let message = format!("Fragment \"{}\" is never used.", frag_info.name);

                // Span the `fragment` keyword (8 bytes from def_start) in the
                // document's coordinate space, then promote to file-level
                // coordinates if the block has a declaration_range.
                let keyword_doc_start = frag_info.def_start;
                let keyword_doc_end = frag_info.def_start + FRAGMENT_KEYWORD_LEN;

                let (diag_span, fix) =
                    if frag_info.block_def_count == 1 && frag_info.declaration_range.is_some() {
                        let (decl_start, decl_end) = frag_info.declaration_range.unwrap();
                        let byte_offset = frag_info.name_span.byte_offset;

                        // File-level keyword span for the diagnostic underline.
                        let file_span = graphql_syntax::SourceSpan {
                            start: byte_offset + keyword_doc_start,
                            end: byte_offset + keyword_doc_end,
                            source: None,
                            line_offset: 0,
                            byte_offset: 0,
                        };

                        let fix = CodeFix::new(
                            format!("Remove unused fragment '{}'", frag_info.name),
                            vec![TextEdit::delete(decl_start, decl_end)],
                        );

                        (file_span, fix)
                    } else {
                        // Document-relative keyword span. Reuse `name_span` to
                        // inherit block context (line_offset / source) for
                        // embedded TS/JS fragments — only the start/end need
                        // to point at the `fragment` keyword instead of the
                        // name.
                        let mut keyword_span = frag_info.name_span.clone();
                        keyword_span.start = keyword_doc_start;
                        keyword_span.end = keyword_doc_end;

                        let fix = CodeFix::new(
                            format!("Remove unused fragment '{}'", frag_info.name),
                            vec![TextEdit::delete(frag_info.def_start, frag_info.def_end)],
                        );
                        (keyword_span, fix)
                    };

                let diag = LintDiagnostic::warning(diag_span, message, "noUnusedFragments")
                    .with_fix(fix)
                    .with_help("Remove unused fragments or reference them in an operation")
                    .with_tag(crate::diagnostics::DiagnosticTag::Unnecessary);

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
    fn test_unused_fragment_has_fix() {
        let db = RootDatabase::default();
        let rule = NoUnusedFragmentsRuleImpl;

        let source = "fragment UnusedFields on User { name }";
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
        let rule = NoUnusedFragmentsRuleImpl;

        let source =
            "fragment UserFields on User { name } query GetUser { user { ...UserFields } }";
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

        // No diagnostics - fragment is used
        assert!(diagnostics.is_empty() || diagnostics.get(&file_id).is_none_or(Vec::is_empty));
    }

    #[test]
    fn test_fragment_used_in_another_file() {
        let db = RootDatabase::default();
        let rule = NoUnusedFragmentsRuleImpl;

        // File 1: Fragment definition
        let source1 = "fragment UserFields on User { name }";
        let file_id1 = FileId::new(0);
        let content1 = FileContent::new(&db, Arc::from(source1));
        let metadata1 = FileMetadata::new(
            &db,
            file_id1,
            FileUri::new("file:///fragments.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // File 2: Operation using the fragment
        let source2 = "query GetUser { user { ...UserFields } }";
        let file_id2 = FileId::new(1);
        let content2 = FileContent::new(&db, Arc::from(source2));
        let metadata2 = FileMetadata::new(
            &db,
            file_id2,
            FileUri::new("file:///queries.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
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
        let rule = NoUnusedFragmentsRuleImpl;

        let source = "fragment A on User { name } fragment B on User { email }";
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
        let rule = NoUnusedFragmentsRuleImpl;

        // Fragment starts after the query - "query Q { user }" has a valid selection
        let source = "query Q { user } fragment Unused on User { name email }";
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
    fn test_ts_single_fragment_fix_deletes_declaration() {
        let db = RootDatabase::default();
        let rule = NoUnusedFragmentsRuleImpl;

        let source =
            "import { gql } from 'graphql-tag';\nconst F = gql`fragment Unused on User { name }`;\n";
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.ts"),
            Language::TypeScript,
            DocumentKind::Executable,
        );

        let project_files = create_test_project_files(&db, &[(file_id, content, metadata)]);
        let diagnostics = rule.check(&db, project_files, None);

        let file_diags = diagnostics
            .get(&file_id)
            .expect("Expected diagnostics for file");
        assert_eq!(file_diags.len(), 1);

        let diag = &file_diags[0];
        assert!(diag.has_fix());
        let fix = diag.fix.as_ref().unwrap();

        // Fix should delete the entire TS declaration, not just the GraphQL content
        let deleted = &source[fix.edits[0].offset_range.start..fix.edits[0].offset_range.end];
        assert!(
            deleted.contains("const F"),
            "Fix should delete the TS declaration, got: {deleted:?}",
        );
    }

    #[test]
    fn test_diagnostic_range_points_to_fragment_name() {
        let db = RootDatabase::default();
        let rule = NoUnusedFragmentsRuleImpl;

        let source = "fragment UnusedFragment on User { name }";
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

        let file_diags = diagnostics
            .get(&file_id)
            .expect("Expected diagnostics for file");
        assert_eq!(file_diags.len(), 1);

        let diag = &file_diags[0];
        // Diagnostic range now points at the `fragment` keyword for parity
        // with graphql-eslint's adapter (which re-locates onto the first
        // token of the fragment definition).
        let keyword_text = &source[diag.span.start..diag.span.end];
        assert_eq!(keyword_text, "fragment");
    }
}
