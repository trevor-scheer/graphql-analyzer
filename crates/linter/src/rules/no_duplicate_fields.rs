use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that detects duplicate fields in selection sets
///
/// When the same field (with the same alias) is selected multiple times in a
/// selection set, it's usually a mistake. This rule reports such duplicates.
///
/// Example:
/// ```graphql
/// # Bad - duplicate field
/// query GetUser {
///   user {
///     id
///     name
///     name  # duplicate
///   }
/// }
///
/// # Good - no duplicates
/// query GetUser {
///   user {
///     id
///     name
///   }
/// }
/// ```
pub struct NoDuplicateFieldsRuleImpl;

impl LintRule for NoDuplicateFieldsRuleImpl {
    fn name(&self) -> &'static str {
        "no_duplicate_fields"
    }

    fn description(&self) -> &'static str {
        "Disallows duplicate fields within the same selection set"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for NoDuplicateFieldsRuleImpl {
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
            for definition in doc_cst.definitions() {
                match definition {
                    cst::Definition::OperationDefinition(op) => {
                        if let Some(selection_set) = op.selection_set() {
                            check_selection_set(&selection_set, &doc, &mut diagnostics);
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        if let Some(selection_set) = frag.selection_set() {
                            check_selection_set(&selection_set, &doc, &mut diagnostics);
                        }
                    }
                    _ => {}
                }
            }
        }

        diagnostics
    }
}

fn check_selection_set(
    selection_set: &cst::SelectionSet,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Track field names (or aliases) -> first occurrence offset
    let mut seen_fields: HashMap<String, usize> = HashMap::new();

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                // Use alias if present, otherwise field name
                let response_name = field
                    .alias()
                    .and_then(|a| a.name())
                    .or_else(|| field.name())
                    .map(|n| n.text().to_string());

                if let Some(name) = response_name {
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        seen_fields.entry(name.clone())
                    {
                        let offset = field
                            .name()
                            .map_or(0, |n| n.syntax().text_range().start().into());
                        e.insert(offset);
                    } else {
                        // Duplicate found
                        let name_node = field
                            .alias()
                            .and_then(|a| a.name())
                            .or_else(|| field.name());

                        if let Some(name_node) = name_node {
                            let start: usize = name_node.syntax().text_range().start().into();
                            let end: usize = name_node.syntax().text_range().end().into();
                            diagnostics.push(LintDiagnostic::new(
                                doc.span(start, end),
                                LintSeverity::Warning,
                                format!("Field '{name}' is already selected in this selection set"),
                                "no_duplicate_fields",
                            ));
                        }
                    }
                }

                // Recurse into nested selection sets
                if let Some(nested) = field.selection_set() {
                    check_selection_set(&nested, doc, diagnostics);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    check_selection_set(&nested, doc, diagnostics);
                }
            }
            cst::Selection::FragmentSpread(_) => {
                // Fragment spreads are checked in their own definitions
            }
        }
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
        let rule = NoDuplicateFieldsRuleImpl;
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
    fn test_no_duplicates() {
        let diagnostics = check("query Q { user { id name email } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_duplicate_field() {
        let diagnostics = check("query Q { user { id name name } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("name"));
    }

    #[test]
    fn test_duplicate_with_alias() {
        // Different aliases for same field are OK (different response names)
        let diagnostics = check("query Q { user { firstName: name lastName: name } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_duplicate_alias() {
        let diagnostics = check("query Q { user { a: name a: email } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("'a'"));
    }

    #[test]
    fn test_nested_duplicates() {
        let diagnostics = check("query Q { user { posts { id id } } }");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_fragment_duplicates() {
        let diagnostics = check("fragment F on User { id name name }");
        assert_eq!(diagnostics.len(), 1);
    }
}
