use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that disallows scalar return types on mutation fields
///
/// Mutations that return scalar types (like Boolean or String) are a GraphQL
/// anti-pattern. They make it impossible for clients to refetch or normalize
/// the mutation result. Mutations should return object types instead.
pub struct NoScalarResultTypeOnMutationRuleImpl;

impl LintRule for NoScalarResultTypeOnMutationRuleImpl {
    fn name(&self) -> &'static str {
        "no_scalar_result_type_on_mutation"
    }

    fn description(&self) -> &'static str {
        "Disallows scalar return types on mutation fields"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for NoScalarResultTypeOnMutationRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        let Some(mutation_type_name) = root_type_names.mutation else {
            return diagnostics_by_file;
        };

        let Some(mutation_type) = schema_types.get(mutation_type_name.as_str()) else {
            return diagnostics_by_file;
        };

        // Built-in scalars
        let builtin_scalars: std::collections::HashSet<&str> =
            ["String", "Int", "Float", "Boolean", "ID"]
                .iter()
                .copied()
                .collect();

        for field in &mutation_type.fields {
            let return_type_name = field.type_ref.name.as_ref();

            // Check if the return type is a scalar (built-in or custom)
            let is_scalar = builtin_scalars.contains(return_type_name)
                || schema_types
                    .get(return_type_name)
                    .is_some_and(|t| t.kind == TypeDefKind::Scalar);

            if is_scalar {
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
                    .entry(mutation_type.file_id)
                    .or_default()
                    .push(LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!(
                            "Mutation field '{}' returns scalar type '{}'. Mutations should return object types.",
                            field.name, return_type_name
                        ),
                        "no_scalar_result_type_on_mutation",
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
    fn test_mutation_returns_object() {
        let db = RootDatabase::default();
        let rule = NoScalarResultTypeOnMutationRuleImpl;
        let schema =
            "type Query { user: User } type Mutation { createUser: User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_mutation_returns_scalar() {
        let db = RootDatabase::default();
        let rule = NoScalarResultTypeOnMutationRuleImpl;
        let schema = "type Query { user: User } type Mutation { deleteUser: Boolean! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("deleteUser"));
    }
}
