use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Convention for names
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NamingCase {
    /// camelCase
    Camel,
    /// `PascalCase`
    Pascal,
    /// `snake_case`
    #[serde(alias = "snake_case")]
    Snake,
    /// `UPPER_CASE`
    #[serde(alias = "UPPER_CASE")]
    Upper,
}

impl NamingCase {
    fn check(self, name: &str) -> bool {
        match self {
            NamingCase::Camel => is_camel_case(name),
            NamingCase::Pascal => is_pascal_case(name),
            NamingCase::Snake => is_snake_case(name),
            NamingCase::Upper => is_upper_case(name),
        }
    }

    fn label(self) -> &'static str {
        match self {
            NamingCase::Camel => "camelCase",
            NamingCase::Pascal => "PascalCase",
            NamingCase::Snake => "snake_case",
            NamingCase::Upper => "UPPER_CASE",
        }
    }
}

fn is_camel_case(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let first = s.chars().next().unwrap();
    first.is_lowercase() && !s.contains('_')
}

fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let first = s.chars().next().unwrap();
    first.is_uppercase() && !s.contains('_')
}

fn is_snake_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_upper_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Options for the `naming_convention` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NamingConventionOptions {
    /// Convention for operation names (default: `PascalCase`)
    #[serde(rename = "OperationDefinition")]
    pub operation_definition: Option<NamingCase>,
    /// Convention for fragment names (default: `PascalCase`)
    #[serde(rename = "FragmentDefinition")]
    pub fragment_definition: Option<NamingCase>,
    /// Convention for variable names (default: camelCase, prefixed with $)
    pub variable: Option<NamingCase>,
}

impl Default for NamingConventionOptions {
    fn default() -> Self {
        Self {
            operation_definition: Some(NamingCase::Pascal),
            fragment_definition: Some(NamingCase::Pascal),
            variable: Some(NamingCase::Camel),
        }
    }
}

impl NamingConventionOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces naming conventions for operations and fragments
pub struct NamingConventionRuleImpl;

impl LintRule for NamingConventionRuleImpl {
    fn name(&self) -> &'static str {
        "naming_convention"
    }

    fn description(&self) -> &'static str {
        "Enforces naming conventions for operations, fragments, and variables"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for NamingConventionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let opts = NamingConventionOptions::from_json(options);
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
                        if let (Some(convention), Some(name_node)) =
                            (opts.operation_definition, op.name())
                        {
                            let name = name_node.text();
                            if !convention.check(&name) {
                                let start: usize = name_node.syntax().text_range().start().into();
                                let end: usize = name_node.syntax().text_range().end().into();
                                diagnostics.push(LintDiagnostic::new(
                                    doc.span(start, end),
                                    LintSeverity::Warning,
                                    format!(
                                        "Operation name '{name}' should be in {} format",
                                        convention.label()
                                    ),
                                    "naming_convention",
                                ));
                            }
                        }

                        // Check variable definitions
                        if let Some(convention) = opts.variable {
                            if let Some(var_defs) = op.variable_definitions() {
                                for var_def in var_defs.variable_definitions() {
                                    if let Some(var) = var_def.variable() {
                                        if let Some(name_node) = var.name() {
                                            let name = name_node.text();
                                            if !convention.check(&name) {
                                                let start: usize =
                                                    name_node.syntax().text_range().start().into();
                                                let end: usize =
                                                    name_node.syntax().text_range().end().into();
                                                diagnostics.push(LintDiagnostic::new(
                                                    doc.span(start, end),
                                                    LintSeverity::Warning,
                                                    format!(
                                                        "Variable '${name}' should be in {} format",
                                                        convention.label()
                                                    ),
                                                    "naming_convention",
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        if let (Some(convention), Some(frag_name)) = (
                            opts.fragment_definition,
                            frag.fragment_name().and_then(|fn_| fn_.name()),
                        ) {
                            let name = frag_name.text();
                            if !convention.check(&name) {
                                let start: usize = frag_name.syntax().text_range().start().into();
                                let end: usize = frag_name.syntax().text_range().end().into();
                                diagnostics.push(LintDiagnostic::new(
                                    doc.span(start, end),
                                    LintSeverity::Warning,
                                    format!(
                                        "Fragment name '{name}' should be in {} format",
                                        convention.label()
                                    ),
                                    "naming_convention",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
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
        let rule = NamingConventionRuleImpl;
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
    fn test_valid_operation_name() {
        let diagnostics = check("query GetUser { user { id } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_operation_name() {
        let diagnostics = check("query get_user { user { id } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PascalCase"));
    }

    #[test]
    fn test_valid_fragment_name() {
        let diagnostics = check("fragment UserFields on User { id }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_fragment_name() {
        let diagnostics = check("fragment user_fields on User { id }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PascalCase"));
    }

    #[test]
    fn test_valid_variable_name() {
        let diagnostics = check("query Q($userId: ID!) { user(id: $userId) { id } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_variable_name() {
        let diagnostics = check("query Q($UserId: ID!) { user(id: $UserId) { id } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("camelCase"));
    }
}
