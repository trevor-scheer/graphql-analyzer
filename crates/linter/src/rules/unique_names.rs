use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::{FragmentNameInfo, OperationNameInfo};
use std::collections::HashMap;

/// Trait implementation for `unique_names` rule
pub struct UniqueNamesRuleImpl;

impl LintRule for UniqueNamesRuleImpl {
    fn name(&self) -> &'static str {
        "unique_names"
    }

    fn description(&self) -> &'static str {
        "Ensures operation and fragment names are unique across the project"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl ProjectLintRule for UniqueNamesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Collect all operations with their locations using per-file cached queries
        let doc_ids = project_files.document_file_ids(db).ids(db);
        let mut operations_by_name: HashMap<String, Vec<(FileId, OperationNameInfo)>> =
            HashMap::new();

        for file_id in doc_ids.iter() {
            // Use per-file lookup for granular caching
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
            let op_names = graphql_hir::file_operation_names(db, *file_id, content, metadata);
            for op_info in op_names.iter() {
                operations_by_name
                    .entry(op_info.name.to_string())
                    .or_default()
                    .push((*file_id, op_info.clone()));
            }
        }

        // Check for duplicate operation names
        for (name, locations) in &operations_by_name {
            if locations.len() > 1 {
                // Found duplicate operation names
                for (file_id, op_info) in locations {
                    let message = format!(
                        "Operation name '{name}' is not unique across the project. Found {} definitions.",
                        locations.len()
                    );

                    // Use the actual name range if available, otherwise fall back to start of file
                    let offset_range = op_info.name_range.map_or_else(
                        || crate::diagnostics::OffsetRange::new(0, name.len()),
                        |range| {
                            crate::diagnostics::OffsetRange::new(
                                range.start().into(),
                                range.end().into(),
                            )
                        },
                    );

                    let mut diag = LintDiagnostic::new(
                        offset_range,
                        self.default_severity(),
                        message,
                        self.name().to_string(),
                    );

                    // For embedded GraphQL, add block context for proper position calculation
                    if let (Some(line_offset), Some(byte_offset), Some(source)) = (
                        op_info.block_line_offset,
                        op_info.block_byte_offset,
                        &op_info.block_source,
                    ) {
                        diag = diag.with_block_context(line_offset, byte_offset, source.clone());
                    }

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        // Collect all fragments with their locations using per-file cached queries
        let mut fragments_by_name: HashMap<String, Vec<(FileId, FragmentNameInfo)>> =
            HashMap::new();

        for file_id in doc_ids.iter() {
            // Use per-file lookup for granular caching
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
            let frag_info = graphql_hir::file_fragment_info(db, *file_id, content, metadata);
            for info in frag_info.iter() {
                fragments_by_name
                    .entry(info.name.to_string())
                    .or_default()
                    .push((*file_id, info.clone()));
            }
        }

        // Check for duplicate fragment names
        for (name, locations) in &fragments_by_name {
            if locations.len() > 1 {
                // Found duplicate fragment names
                for (file_id, frag_info) in locations {
                    let message = format!(
                        "Fragment name '{name}' is not unique across the project. Found {} definitions.",
                        locations.len()
                    );

                    // Use the actual name range
                    let offset_range = crate::diagnostics::OffsetRange::new(
                        frag_info.name_range.start().into(),
                        frag_info.name_range.end().into(),
                    );

                    let mut diag = LintDiagnostic::new(
                        offset_range,
                        self.default_severity(),
                        message,
                        self.name().to_string(),
                    );

                    // For embedded GraphQL, add block context for proper position calculation
                    if let (Some(line_offset), Some(byte_offset), Some(source)) = (
                        frag_info.block_line_offset,
                        frag_info.block_byte_offset,
                        &frag_info.block_source,
                    ) {
                        diag = diag.with_block_context(line_offset, byte_offset, source.clone());
                    }

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        diagnostics_by_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::ProjectLintRule;
    use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_multi_file_project(
        db: &dyn graphql_hir::GraphQLHirDatabase,
        documents: &[(&str, &str)],
    ) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));

        let mut file_entries = std::collections::HashMap::new();
        let mut doc_file_ids = Vec::new();

        #[allow(clippy::cast_possible_truncation)]
        for (i, (uri, source)) in documents.iter().enumerate() {
            let file_id = FileId::new(i as u32);
            let content = FileContent::new(db, Arc::from(*source));
            let metadata =
                FileMetadata::new(db, file_id, FileUri::new(*uri), FileKind::ExecutableGraphQL);

            let entry = graphql_base_db::FileEntry::new(db, content, metadata);
            file_entries.insert(file_id, entry);
            doc_file_ids.push(file_id);
        }

        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(doc_file_ids));
        let file_entry_map = graphql_base_db::FileEntryMap::new(db, Arc::new(file_entries));

        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    fn create_single_file_project(
        db: &dyn graphql_hir::GraphQLHirDatabase,
        source: &str,
    ) -> ProjectFiles {
        create_multi_file_project(db, &[("file:///test.graphql", source)])
    }

    #[test]
    fn test_unique_operation_names_no_warning() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
query GetUser { user { id } }
query GetPosts { posts { id } }
mutation UpdateUser { updateUser { id } }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_duplicate_operation_names_in_same_file() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
query GetUser { user { id } }
query GetUser { user { name } }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        let file_diags = diagnostics.values().next().unwrap();
        assert_eq!(file_diags.len(), 2);
        assert!(file_diags[0].message.contains("GetUser"));
        assert!(file_diags[0].message.contains("not unique"));
    }

    #[test]
    fn test_duplicate_operation_names_across_files() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let documents = [
            ("file:///file1.graphql", "query GetUser { user { id } }"),
            ("file:///file2.graphql", "query GetUser { user { name } }"),
        ];

        let project_files = create_multi_file_project(&db, &documents);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 2);
        let total_diags: usize = diagnostics.values().map(Vec::len).sum();
        assert_eq!(total_diags, 2);
    }

    #[test]
    fn test_unique_fragment_names_no_warning() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
fragment UserFields on User { id name }
fragment PostFields on Post { id title }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_duplicate_fragment_names_in_same_file() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
fragment UserFields on User { id name }
fragment UserFields on User { id email }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        let file_diags = diagnostics.values().next().unwrap();
        assert_eq!(file_diags.len(), 2);
        assert!(file_diags[0].message.contains("UserFields"));
        assert!(file_diags[0].message.contains("not unique"));
    }

    #[test]
    fn test_duplicate_fragment_names_across_files() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let documents = [
            (
                "file:///fragments1.graphql",
                "fragment UserFields on User { id }",
            ),
            (
                "file:///fragments2.graphql",
                "fragment UserFields on User { name }",
            ),
        ];

        let project_files = create_multi_file_project(&db, &documents);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 2);
        let total_diags: usize = diagnostics.values().map(Vec::len).sum();
        assert_eq!(total_diags, 2);
    }

    #[test]
    fn test_same_name_for_operation_and_fragment_allowed() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
query UserFields { user { id } }
fragment UserFields on User { id name }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_three_duplicate_operation_names() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
query GetUser { user { id } }
query GetUser { user { name } }
query GetUser { user { email } }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        let file_diags = diagnostics.values().next().unwrap();
        assert_eq!(file_diags.len(), 3);
        assert!(file_diags[0].message.contains("3 definitions"));
    }

    #[test]
    fn test_anonymous_operations_not_checked() {
        let db = RootDatabase::default();
        let rule = UniqueNamesRuleImpl;

        let source = r"
{ user { id } }
{ posts { id } }
";

        let project_files = create_single_file_project(&db, source);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(diagnostics.is_empty());
    }
}
