use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_apollo_ext::{DocumentExt, NameExt, RangeExt};
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
                collect_fragment_definitions(doc.tree, *file_id, &mut all_fragments);
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
    tree: &apollo_parser::SyntaxTree,
    file_id: FileId,
    fragments: &mut Vec<FragmentInfo>,
) {
    for frag in tree.fragments() {
        let Some(name) = frag.name_text() else {
            continue;
        };
        let Some(name_range) = frag.name_range() else {
            continue;
        };
        let def_range = frag.byte_range();

        fragments.push(FragmentInfo {
            name,
            file_id,
            name_start: name_range.start,
            name_end: name_range.end,
            def_start: def_range.start,
            def_end: def_range.end,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::ProjectLintRule;
    use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};
    use graphql_hir::GraphQLHirDatabase;
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    /// Helper to create test project files with schema and document
    fn create_test_project(
        db: &dyn GraphQLHirDatabase,
        schema_source: &str,
        document_source: &str,
        document_kind: FileKind,
    ) -> (FileId, FileContent, FileMetadata, ProjectFiles) {
        // Create schema file
        let schema_file_id = FileId::new(0);
        let schema_content = FileContent::new(db, Arc::from(schema_source));
        let schema_metadata = FileMetadata::new(
            db,
            schema_file_id,
            FileUri::new("file:///schema.graphql"),
            FileKind::Schema,
        );

        // Create document file
        let doc_file_id = FileId::new(1);
        let doc_content = FileContent::new(db, Arc::from(document_source));
        let doc_metadata = FileMetadata::new(
            db,
            doc_file_id,
            FileUri::new("file:///query.ts"),
            document_kind,
        );

        let schema_file_ids =
            graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![schema_file_id]));
        let document_file_ids =
            graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![doc_file_id]));
        let mut file_entries = std::collections::HashMap::new();
        let schema_entry = graphql_base_db::FileEntry::new(db, schema_content, schema_metadata);
        let doc_entry = graphql_base_db::FileEntry::new(db, doc_content, doc_metadata);
        file_entries.insert(schema_file_id, schema_entry);
        file_entries.insert(doc_file_id, doc_entry);
        let file_entry_map = graphql_base_db::FileEntryMap::new(db, Arc::new(file_entries));
        let project_files =
            ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map);

        (doc_file_id, doc_content, doc_metadata, project_files)
    }

    const TEST_SCHEMA: &str = r"
type Query {
    user(id: ID!): User
}

type User {
    id: ID!
    name: String!
}
";

    #[test]
    fn test_unused_fragment_in_typescript_has_correct_offset() {
        // This test reproduces issue #450: unused_fragments diagnostics use wrong offset
        // in embedded GraphQL. The diagnostic should point to the correct position in
        // the TypeScript file, accounting for the line offset of the gql template literal.
        let db = RootDatabase::default();
        let rule = UnusedFragmentsRuleImpl;

        // TypeScript source with embedded GraphQL.
        // The fragment starts on line 4 (0-indexed: line 3), column 4.
        // Lines 0-2 are: import, empty line, const declaration
        // Line 3 starts with "    fragment UnusedFragment..."
        let source = r#"import { gql } from '@apollo/client';

const UNUSED = gql`
    fragment UnusedFragment on User {
        id
        name
    }
`;
"#;

        let (_doc_file_id, _content, _metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::TypeScript);

        let diagnostics_by_file = rule.check(&db, project_files, None);

        // Should have exactly one diagnostic for the unused fragment
        assert_eq!(diagnostics_by_file.len(), 1, "Expected diagnostics for one file");
        let diagnostics = diagnostics_by_file.values().next().unwrap();
        assert_eq!(diagnostics.len(), 1, "Expected one diagnostic");

        let diag = &diagnostics[0];
        assert!(
            diag.message.contains("UnusedFragment"),
            "Diagnostic should mention the fragment name"
        );

        // For embedded GraphQL in TypeScript, the diagnostic should have block context set.
        // The block_byte_offset indicates where the GraphQL block starts in the TypeScript file.
        // This is essential for correctly positioning diagnostics in the original file.
        //
        // If the bug exists (issue #450), block_byte_offset will be None because the
        // rule doesn't propagate the block context when collecting fragments.
        // If fixed, block_byte_offset should be Some with a value > 0.
        let gql_content_start = source.find("gql`").expect("source should contain gql`") + 4; // +4 for "gql`"

        assert!(
            diag.block_byte_offset.is_some(),
            "Diagnostic for embedded GraphQL should have block_byte_offset set. \
             This is None, indicating the rule is not propagating block context (issue #450)."
        );

        let block_byte_offset = diag.block_byte_offset.unwrap();
        assert!(
            block_byte_offset >= gql_content_start,
            "block_byte_offset {} should be >= gql content position {} (after gql`). \
             The diagnostic should know the GraphQL block's position in the TypeScript file.",
            block_byte_offset,
            gql_content_start
        );

        // Also verify block_line_offset is set (the line number where the GraphQL block starts)
        assert!(
            diag.block_line_offset.is_some(),
            "Diagnostic for embedded GraphQL should have block_line_offset set."
        );

        // The gql template literal starts on line 2 (0-indexed), so line_offset should be >= 2
        let block_line_offset = diag.block_line_offset.unwrap();
        assert!(
            block_line_offset >= 2,
            "block_line_offset {} should be >= 2 (the line where gql` content starts).",
            block_line_offset
        );
    }
}
