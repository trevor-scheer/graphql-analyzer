use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};

/// Lint rule that requires each file to contain only one executable definition
///
/// Having one operation or fragment per file improves code organization and
/// makes it easier to find and maintain GraphQL operations.
pub struct LoneExecutableDefinitionRuleImpl;

impl LintRule for LoneExecutableDefinitionRuleImpl {
    fn name(&self) -> &'static str {
        "lone_executable_definition"
    }

    fn description(&self) -> &'static str {
        "Requires each file to contain only one executable definition (operation or fragment)"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for LoneExecutableDefinitionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            let mut operations = Vec::new();
            let mut fragments = Vec::new();

            for definition in doc_cst.definitions() {
                match &definition {
                    cst::Definition::OperationDefinition(op) => {
                        operations.push(op.clone());
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        fragments.push(frag.clone());
                    }
                    _ => {}
                }
            }

            let total_defs = operations.len() + fragments.len();
            if total_defs <= 1 {
                continue;
            }

            // Report all definitions after the first one
            let mut all_defs: Vec<(&str, Option<String>, usize, usize)> = Vec::new();

            for op in &operations {
                let name = op.name().map(|n| n.text().to_string());
                let name_or_keyword = op
                    .name()
                    .map(|n| {
                        let start: usize = n.syntax().text_range().start().into();
                        let end: usize = n.syntax().text_range().end().into();
                        (start, end)
                    })
                    .or_else(|| {
                        op.operation_type().map(|ot| {
                            let start: usize = ot.syntax().text_range().start().into();
                            let end: usize = ot.syntax().text_range().end().into();
                            (start, end)
                        })
                    })
                    .or_else(|| {
                        op.selection_set().map(|ss| {
                            let start: usize = ss.syntax().text_range().start().into();
                            (start, start + 1)
                        })
                    });

                if let Some((start, end)) = name_or_keyword {
                    all_defs.push(("operation", name, start, end));
                }
            }

            for frag in &fragments {
                let name = frag
                    .fragment_name()
                    .and_then(|fn_| fn_.name())
                    .map(|n| n.text().to_string());
                let name_or_keyword = frag.fragment_name().and_then(|fn_| fn_.name()).map(|n| {
                    let start: usize = n.syntax().text_range().start().into();
                    let end: usize = n.syntax().text_range().end().into();
                    (start, end)
                });

                if let Some((start, end)) = name_or_keyword {
                    all_defs.push(("fragment", name, start, end));
                }
            }

            // Sort by position and skip the first definition
            all_defs.sort_by_key(|d| d.2);
            for (kind, name, start, end) in all_defs.into_iter().skip(1) {
                let def_desc =
                    name.map_or_else(|| format!("anonymous {kind}"), |n| format!("{kind} '{n}'"));
                diagnostics.push(LintDiagnostic::new(
                    doc.span(start, end),
                    LintSeverity::Warning,
                    format!(
                        "Only one executable definition is allowed per file. Found additional {def_desc}."
                    ),
                    "lone_executable_definition",
                ));
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneDocumentLintRule;
    use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = LoneExecutableDefinitionRuleImpl;
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);
        rule.check(&db, file_id, content, metadata, project_files, None)
    }

    #[test]
    fn test_single_operation() {
        let diagnostics = check("query Q { user { id } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_single_fragment() {
        let diagnostics = check("fragment F on User { id }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_multiple_operations() {
        let diagnostics = check("query Q1 { user { id } } query Q2 { posts { id } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Q2"));
    }

    #[test]
    fn test_operation_and_fragment() {
        let diagnostics = check("fragment F on User { id } query Q { user { ...F } }");
        assert_eq!(diagnostics.len(), 1);
    }
}
