use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use serde::Deserialize;
use std::collections::HashMap;

/// The expected description style
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DescriptionStyleKind {
    /// Inline string: "description"
    Inline,
    /// Block string: """description"""
    Block,
}

/// Options for the `description_style` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DescriptionStyleOptions {
    /// The expected style for descriptions
    pub style: DescriptionStyleKind,
}

impl Default for DescriptionStyleOptions {
    fn default() -> Self {
        Self {
            style: DescriptionStyleKind::Block,
        }
    }
}

impl DescriptionStyleOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces consistent description style in schema
pub struct DescriptionStyleRuleImpl;

impl LintRule for DescriptionStyleRuleImpl {
    fn name(&self) -> &'static str {
        "description_style"
    }

    fn description(&self) -> &'static str {
        "Enforces consistent description style (block vs inline)"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for DescriptionStyleRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = DescriptionStyleOptions::from_json(options);
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
                for definition in &doc.ast.definitions {
                    let desc = match definition {
                        apollo_compiler::ast::Definition::ObjectTypeDefinition(d) => {
                            d.description.as_ref()
                        }
                        apollo_compiler::ast::Definition::InterfaceTypeDefinition(d) => {
                            d.description.as_ref()
                        }
                        apollo_compiler::ast::Definition::UnionTypeDefinition(d) => {
                            d.description.as_ref()
                        }
                        apollo_compiler::ast::Definition::EnumTypeDefinition(d) => {
                            d.description.as_ref()
                        }
                        apollo_compiler::ast::Definition::ScalarTypeDefinition(d) => {
                            d.description.as_ref()
                        }
                        apollo_compiler::ast::Definition::InputObjectTypeDefinition(d) => {
                            d.description.as_ref()
                        }
                        _ => continue,
                    };

                    let Some(desc) = desc else {
                        continue;
                    };

                    let desc_str = desc.as_str();
                    let desc_location = desc.location();

                    // Determine the actual style used by checking source text
                    let Some(location) = desc_location else {
                        continue;
                    };
                    let start = location.offset();
                    let end = location.end_offset();

                    let source = doc.source;
                    if start >= source.len() || end > source.len() {
                        continue;
                    }

                    let raw = &source[start..end];
                    let is_block = raw.starts_with("\"\"\"");

                    let wrong_style = match opts.style {
                        DescriptionStyleKind::Block => !is_block,
                        DescriptionStyleKind::Inline => is_block,
                    };

                    if wrong_style {
                        let expected = match opts.style {
                            DescriptionStyleKind::Block => "block (\"\"\"...\"\"\")",
                            DescriptionStyleKind::Inline => "inline (\"...\")",
                        };

                        diagnostics_by_file
                            .entry(*file_id)
                            .or_default()
                            .push(LintDiagnostic::new(
                                doc.span(start, end.min(start + desc_str.len().min(30) + 6)),
                                LintSeverity::Warning,
                                format!("Description should use {expected} style"),
                                "description_style",
                            ));
                    }
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
    fn test_block_style_with_block_option() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""A user"""
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None); // default is block
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_inline_style_with_block_option() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"A user"
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None); // default is block
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("block"));
    }
}
