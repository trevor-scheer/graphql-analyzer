use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that requires an ID field in object types
///
/// Object types should include an `id` field of type `ID` to enable
/// proper caching and normalization in GraphQL clients.
pub struct StrictIdInTypesRuleImpl;

impl LintRule for StrictIdInTypesRuleImpl {
    fn name(&self) -> &'static str {
        "strict_id_in_types"
    }

    fn description(&self) -> &'static str {
        "Requires object types to have an ID field"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for StrictIdInTypesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Determine root type names to exclude them
        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::Object {
                continue;
            }

            // Skip root types (Query, Mutation, Subscription)
            if root_type_names.is_root_type(&type_def.name) {
                continue;
            }

            // Check if the type has a field named "id" of type "ID"
            let has_id_field = type_def
                .fields
                .iter()
                .any(|f| f.name.as_ref() == "id" && f.type_ref.name.as_ref() == "ID");

            if !has_id_field {
                let start: usize = type_def.name_range.start().into();
                let end: usize = type_def.name_range.end().into();
                let span = graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };

                diagnostics_by_file
                    .entry(type_def.file_id)
                    .or_default()
                    .push(LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!(
                            "Type '{}' is missing an 'id' field of type 'ID'",
                            type_def.name
                        ),
                        "strict_id_in_types",
                    ));
            }
        }

        diagnostics_by_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneSchemaLintRule;
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, ProjectFiles, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_schema_project(db: &RootDatabase, schema: &str) -> ProjectFiles {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(schema));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let entry = FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![file_id]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_type_with_id() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_type_without_id() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type User { name: String! email: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("'User'"));
    }

    #[test]
    fn test_query_type_excluded() {
        let db = RootDatabase::default();
        let rule = StrictIdInTypesRuleImpl;
        let schema = "type Query { users: [User!]! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }
}
