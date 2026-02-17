use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that requires mutation result types to include a Query field
///
/// When mutation results include a field that returns the Query type,
/// clients can refetch any data they need in a single round trip after
/// the mutation completes.
pub struct RequireFieldOfTypeQueryInMutationResultRuleImpl;

impl LintRule for RequireFieldOfTypeQueryInMutationResultRuleImpl {
    fn name(&self) -> &'static str {
        "require_field_of_type_query_in_mutation_result"
    }

    fn description(&self) -> &'static str {
        "Requires mutation result types to include a field of the Query type"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RequireFieldOfTypeQueryInMutationResultRuleImpl {
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

        let Some(query_type_name) = root_type_names.query else {
            return diagnostics_by_file;
        };

        let Some(mutation_type) = schema_types.get(mutation_type_name.as_str()) else {
            return diagnostics_by_file;
        };

        for field in &mutation_type.fields {
            let return_type_name = field.type_ref.name.as_ref();

            // Check if the return type is an object type (skip scalars, enums, etc.)
            let return_type = match schema_types.get(return_type_name) {
                Some(t) if t.kind == TypeDefKind::Object => t,
                _ => continue,
            };

            // Check if the return type has a field that returns the Query type
            let has_query_field = return_type
                .fields
                .iter()
                .any(|f| f.type_ref.name.as_ref() == query_type_name);

            if !has_query_field {
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
                            "Mutation field '{}' result type '{}' should include a field of type '{}'",
                            field.name, return_type_name, query_type_name
                        ),
                        "require_field_of_type_query_in_mutation_result",
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
    fn test_mutation_result_with_query_field() {
        let db = RootDatabase::default();
        let rule = RequireFieldOfTypeQueryInMutationResultRuleImpl;
        let schema = "type Query { user: User } type Mutation { createUser: CreateUserResult! } type CreateUserResult { user: User! query: Query! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_mutation_result_without_query_field() {
        let db = RootDatabase::default();
        let rule = RequireFieldOfTypeQueryInMutationResultRuleImpl;
        let schema = "type Query { user: User } type Mutation { createUser: CreateUserResult! } type CreateUserResult { user: User! } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("createUser"));
    }
}
