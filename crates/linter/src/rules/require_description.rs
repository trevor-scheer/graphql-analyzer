use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that requires descriptions on type definitions
///
/// Descriptions serve as documentation for schema consumers. This rule
/// ensures that all type definitions include a description.
pub struct RequireDescriptionRuleImpl;

impl LintRule for RequireDescriptionRuleImpl {
    fn name(&self) -> &'static str {
        "require_description"
    }

    fn description(&self) -> &'static str {
        "Requires descriptions on type definitions"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RequireDescriptionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            // Skip built-in scalars
            if type_def.kind == TypeDefKind::Scalar
                && matches!(
                    type_def.name.as_ref(),
                    "String" | "Int" | "Float" | "Boolean" | "ID"
                )
            {
                continue;
            }

            if type_def.description.is_none() {
                let kind_name = match type_def.kind {
                    TypeDefKind::Interface => "interface",
                    TypeDefKind::Union => "union",
                    TypeDefKind::Enum => "enum",
                    TypeDefKind::Scalar => "scalar",
                    TypeDefKind::InputObject => "input",
                    _ => "type",
                };

                let start: usize = type_def.name_range.start().into();
                let end: usize = type_def.name_range.end().into();

                // Create a SourceSpan from the schema file
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
                        format!("{kind_name} '{}' is missing a description", type_def.name),
                        "require_description",
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
    fn test_type_with_description() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;

        let schema = r#"
"A user in the system"
type User {
    id: ID!
    name: String!
}
"#;

        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);

        // Should not warn about User (has description) or built-in types
        let user_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("'User'"))
            .collect();
        assert!(user_warnings.is_empty());
    }

    #[test]
    fn test_type_without_description() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;

        let schema = r"
type User {
    id: ID!
    name: String!
}
";

        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);

        let user_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("'User'"))
            .collect();
        assert_eq!(user_warnings.len(), 1);
    }
}
