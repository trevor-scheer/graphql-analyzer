use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that detects duplicate enum value names across different enums
///
/// When the same value name appears in multiple enums, it can cause confusion
/// and make refactoring harder. This rule warns when enum value names collide.
pub struct UniqueEnumValueNamesRuleImpl;

impl LintRule for UniqueEnumValueNamesRuleImpl {
    fn name(&self) -> &'static str {
        "unique_enum_value_names"
    }

    fn description(&self) -> &'static str {
        "Detects duplicate enum value names across different enum types"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for UniqueEnumValueNamesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Collect all enum value -> enum type mappings
        let mut value_to_enums: HashMap<String, Vec<(String, FileId, graphql_hir::TextRange)>> =
            HashMap::new();

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::Enum {
                continue;
            }

            for ev in &type_def.enum_values {
                value_to_enums
                    .entry(ev.name.to_string())
                    .or_default()
                    .push((
                        type_def.name.to_string(),
                        type_def.file_id,
                        type_def.name_range,
                    ));
            }
        }

        // Report values that appear in multiple enums
        for (value_name, enums) in &value_to_enums {
            if enums.len() <= 1 {
                continue;
            }

            for (enum_name, file_id, name_range) in enums {
                let other_enums: Vec<_> = enums
                    .iter()
                    .filter(|(n, _, _)| n != enum_name)
                    .map(|(n, _, _)| n.as_str())
                    .collect();

                let start: usize = name_range.start().into();
                let end: usize = name_range.end().into();
                let span = graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };

                diagnostics_by_file
                    .entry(*file_id)
                    .or_default()
                    .push(LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!(
                            "Enum value '{value_name}' in '{enum_name}' is also defined in: {}",
                            other_enums.join(", ")
                        ),
                        "unique_enum_value_names",
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
    fn test_unique_values() {
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum Status { ACTIVE INACTIVE } enum Role { ADMIN USER }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_duplicate_values() {
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum Status { ACTIVE INACTIVE } enum UserStatus { ACTIVE PENDING }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2); // One for each enum that has "ACTIVE"
    }
}
