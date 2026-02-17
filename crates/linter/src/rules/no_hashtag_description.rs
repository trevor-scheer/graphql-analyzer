use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that disallows using # comments as type descriptions
///
/// In GraphQL SDL, descriptions should use string literals (double quotes or
/// triple quotes), not hash comments. Hash comments are not part of the
/// schema and won't appear in introspection results.
pub struct NoHashtagDescriptionRuleImpl;

impl LintRule for NoHashtagDescriptionRuleImpl {
    fn name(&self) -> &'static str {
        "no_hashtag_description"
    }

    fn description(&self) -> &'static str {
        "Disallows using # comments as type descriptions in schema"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for NoHashtagDescriptionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_ids = project_files.schema_file_ids(db).ids(db);

        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            let parse = graphql_syntax::parse(db, content, metadata);
            if parse.has_errors() {
                continue;
            }

            for doc in parse.documents() {
                let source = doc.source;
                let lines: Vec<&str> = source.lines().collect();

                for (i, line) in lines.iter().enumerate() {
                    let trimmed = line.trim();

                    // Check if this is a comment line
                    if !trimmed.starts_with('#') {
                        continue;
                    }

                    // Check if the next non-empty, non-comment line is a type definition keyword
                    let mut next_idx = i + 1;
                    while next_idx < lines.len() {
                        let next_trimmed = lines[next_idx].trim();
                        if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                            next_idx += 1;
                            continue;
                        }
                        break;
                    }

                    if next_idx < lines.len() {
                        let next_line = lines[next_idx].trim();
                        if is_type_definition_line(next_line) {
                            // Calculate byte offset of this comment line
                            let line_start: usize = lines[..i]
                                .iter()
                                .map(|l| l.len() + 1) // +1 for newline
                                .sum();
                            let comment_start = line_start + line.len() - line.trim_start().len();
                            let comment_end = line_start + line.trim_end().len();

                            diagnostics_by_file.entry(*file_id).or_default().push(
                                LintDiagnostic::new(
                                    doc.span(comment_start, comment_end),
                                    LintSeverity::Warning,
                                    "Use string literals (\"\" or \"\"\"\"\"\") for descriptions instead of # comments. Comments won't appear in introspection.".to_string(),
                                    "no_hashtag_description",
                                ),
                            );
                        }
                    }
                }
            }
        }

        diagnostics_by_file
    }
}

fn is_type_definition_line(line: &str) -> bool {
    let keywords = [
        "type ",
        "interface ",
        "union ",
        "enum ",
        "scalar ",
        "input ",
        "directive ",
        "extend type ",
        "extend interface ",
        "extend union ",
        "extend enum ",
        "extend scalar ",
        "extend input ",
    ];
    keywords.iter().any(|k| line.starts_with(k))
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
    fn test_proper_description() {
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r#"
"A user in the system"
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_hashtag_description() {
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r"
# A user in the system
type User {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("string literals"));
    }

    #[test]
    fn test_inline_comment_not_before_type() {
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r#"
"A user"
type User {
    # internal comment
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }
}
