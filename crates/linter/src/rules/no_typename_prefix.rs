use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that disallows field names prefixed with their type name
///
/// Fields like `User.userName` are redundant since the type context is
/// already known. Prefer `User.name` instead.
pub struct NoTypenamePrefixRuleImpl;

impl LintRule for NoTypenamePrefixRuleImpl {
    fn name(&self) -> &'static str {
        "no_typename_prefix"
    }

    fn description(&self) -> &'static str {
        "Disallows field names that are prefixed with their parent type name"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for NoTypenamePrefixRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            if !matches!(
                type_def.kind,
                TypeDefKind::Object | TypeDefKind::Interface | TypeDefKind::InputObject
            ) {
                continue;
            }

            let type_name_lower = type_def.name.to_lowercase();

            for field in &type_def.fields {
                let field_name_lower = field.name.to_lowercase();

                if field_name_lower.starts_with(&type_name_lower)
                    && field_name_lower.len() > type_name_lower.len()
                {
                    let start: usize = field.name_range.start().into();
                    let end: usize = field.name_range.end().into();
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
                                "Field '{}' on type '{}' starts with the type name. Consider renaming to '{}'.",
                                field.name,
                                type_def.name,
                                &field.name[type_def.name.len()..]
                            ),
                            "no_typename_prefix",
                        ));
                }
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
    fn test_no_prefix() {
        let db = RootDatabase::default();
        let rule = NoTypenamePrefixRuleImpl;
        let schema = "type User { id: ID! name: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_with_prefix() {
        let db = RootDatabase::default();
        let rule = NoTypenamePrefixRuleImpl;
        let schema = "type User { userId: ID! userName: String! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
    }
}
