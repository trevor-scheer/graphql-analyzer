use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that flags case-insensitive duplicate value names within a single enum.
///
/// Mirrors graphql-eslint's `unique-enum-value-names`: any value whose
/// lowercased name collides with an earlier value in the same enum is reported
/// (the first occurrence is left alone).
pub struct UniqueEnumValueNamesRuleImpl;

impl LintRule for UniqueEnumValueNamesRuleImpl {
    fn name(&self) -> &'static str {
        "uniqueEnumValueNames"
    }

    fn description(&self) -> &'static str {
        "Disallows case-insensitive duplicate values within a single enum"
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

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::Enum {
                continue;
            }

            // Track which lowercased names have already been seen in this enum;
            // anything appearing a second-or-later time is a case-insensitive duplicate.
            let mut seen: HashMap<String, usize> = HashMap::new();
            for ev in &type_def.enum_values {
                let key = ev.name.to_lowercase();
                let count = seen.entry(key).or_insert(0);
                *count += 1;
                if *count == 1 {
                    continue;
                }

                // Mirror graphql-eslint: each diagnostic spans the duplicate
                // value's name token (not the enum's name).
                let start: usize = ev.name_range.start().into();
                let end: usize = ev.name_range.end().into();
                let span = graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };

                // Suggestion: remove the duplicate enum value def (matches
                // upstream's `fixer.remove(duplicate)`). Range covers the
                // value's name plus any trailing directives via
                // `EnumValue.definition_range`.
                let def_start: usize = ev.definition_range.start().into();
                let def_end: usize = ev.definition_range.end().into();
                let suggestion = CodeSuggestion::delete(
                    format!("Remove `{}` enum value", ev.name),
                    def_start,
                    def_end,
                );

                diagnostics_by_file.entry(type_def.file_id).or_default().push(
                    LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!(
                            "Unexpected case-insensitive enum values duplicates for enum value \"{}\" in enum \"{}\"",
                            ev.name, type_def.name
                        ),
                        "uniqueEnumValueNames",
                    )
                    .with_suggestion(suggestion),
                );
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
    fn test_unique_values_within_enum() {
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum Status { ACTIVE INACTIVE } enum Role { ADMIN USER }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_cross_enum_duplicates_are_allowed() {
        // graphql-eslint only flags within-enum collisions; identical names across
        // different enums are intentionally not reported.
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum Status { ACTIVE INACTIVE } enum UserStatus { ACTIVE PENDING }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_case_insensitive_duplicate_pair() {
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum E { Active ACTIVE }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0]
            .message
            .contains("enum value \"ACTIVE\" in enum \"E\""));
    }

    #[test]
    fn test_case_insensitive_duplicate_triple() {
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum E { foo FOO Foo }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let mut messages: Vec<_> = diagnostics
            .values()
            .flatten()
            .map(|d| d.message.clone())
            .collect();
        messages.sort();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("enum value \"FOO\" in enum \"E\""));
        assert!(messages[1].contains("enum value \"Foo\" in enum \"E\""));
    }

    #[test]
    fn test_diagnostic_spans_duplicate_value_name() {
        // Mirrors graphql-eslint: each diagnostic spans the duplicate
        // value's name token, not the enum's name.
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        // Offsets:    0         1         2
        //             0123456789012345678901234567
        let schema = "enum E { Value VALUE ValuE }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let mut all: Vec<_> = diagnostics.values().flatten().collect();
        all.sort_by_key(|d| d.span.start);
        assert_eq!(all.len(), 2);

        // VALUE at offsets 15..20
        assert_eq!(all[0].span.start, 15);
        assert_eq!(all[0].span.end, 20);
        assert!(all[0].message.contains("\"VALUE\""));

        // ValuE at offsets 21..26
        assert_eq!(all[1].span.start, 21);
        assert_eq!(all[1].span.end, 26);
        assert!(all[1].message.contains("\"ValuE\""));
    }

    #[test]
    fn test_exact_duplicate_is_also_reported() {
        // Exact-match duplicates are a subset of case-insensitive duplicates.
        let db = RootDatabase::default();
        let rule = UniqueEnumValueNamesRuleImpl;
        let schema = "enum E { VALUE VALUE }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
    }
}
