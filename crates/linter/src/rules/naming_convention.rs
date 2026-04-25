use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::rules::{get_operation_kind, OperationKind};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;

/// Convention for names. Accepts the same string forms as graphql-eslint:
/// `"camelCase"`, `"PascalCase"`, `"snake_case"`, `"UPPER_CASE"`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum NamingCase {
    /// camelCase
    #[serde(rename = "camelCase")]
    Camel,
    /// `PascalCase`
    #[serde(rename = "PascalCase")]
    Pascal,
    /// `snake_case`
    #[serde(rename = "snake_case")]
    Snake,
    /// `UPPER_CASE`
    #[serde(rename = "UPPER_CASE")]
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

/// Options for the `naming_convention` rule.
///
/// Mirrors graphql-eslint: with no options the rule no-ops. Each AST kind
/// must be opted into explicitly.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NamingConventionOptions {
    /// Convention for operation names (no default; must be set to fire)
    #[serde(rename = "OperationDefinition")]
    pub operation_definition: Option<NamingCase>,
    /// Convention for fragment names (no default; must be set to fire)
    #[serde(rename = "FragmentDefinition")]
    pub fragment_definition: Option<NamingCase>,
    /// Convention for variable names (no default; must be set to fire)
    #[serde(rename = "VariableDefinition", alias = "Variable")]
    pub variable: Option<NamingCase>,
}

impl NamingConventionOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces naming conventions for operations and fragments.
///
/// Like graphql-eslint, the rule no-ops with no options — each kind must be
/// explicitly configured.
// TODO(parity): graphql-eslint's `naming-convention` rule additionally
// supports `prefix`, `suffix`, `forbiddenPatterns`, `requiredPattern`,
// `forbiddenPrefixes`/`forbiddenSuffixes`, `requiredPrefixes`/`requiredSuffixes`,
// `ignorePattern`, `allowLeadingUnderscore`/`allowTrailingUnderscore`, the
// `types` umbrella option, ESLint selector keys, and schema-side kinds
// (FieldDefinition, ObjectTypeDefinition, EnumValueDefinition, etc.). Their
// corresponding diagnostic messages (`have "X" prefix`, `not contain the
// forbidden pattern "..."`, `Leading underscores are not allowed`, etc.) are
// not emitted here.
pub struct NamingConventionRuleImpl;

impl LintRule for NamingConventionRuleImpl {
    fn name(&self) -> &'static str {
        "namingConvention"
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
                                let op_kind =
                                    op.operation_type().map_or(OperationKind::Query, |op_type| {
                                        get_operation_kind(&op_type)
                                    });
                                let op_label = match op_kind {
                                    OperationKind::Query => "Query",
                                    OperationKind::Mutation => "Mutation",
                                    OperationKind::Subscription => "Subscription",
                                };
                                let start: usize = name_node.syntax().text_range().start().into();
                                let end: usize = name_node.syntax().text_range().end().into();
                                diagnostics.push(
                                    LintDiagnostic::new(
                                        doc.span(start, end),
                                        LintSeverity::Warning,
                                        format!(
                                            "{op_label} \"{name}\" should be in {} format",
                                            convention.label()
                                        ),
                                        "namingConvention",
                                    )
                                    .with_help(format!(
                                        "Rename the operation to use {} casing",
                                        convention.label()
                                    )),
                                );
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
                                                diagnostics.push(
                                                    LintDiagnostic::new(
                                                        doc.span(start, end),
                                                        LintSeverity::Warning,
                                                        format!(
                                                            "Variable \"{name}\" should be in {} format",
                                                            convention.label()
                                                        ),
                                                        "namingConvention",
                                                    )
                                                    .with_help(format!(
                                                        "Rename the variable to use {} casing",
                                                        convention.label()
                                                    )),
                                                );
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
                                diagnostics.push(
                                    LintDiagnostic::new(
                                        doc.span(start, end),
                                        LintSeverity::Warning,
                                        format!(
                                            "Fragment \"{name}\" should be in {} format",
                                            convention.label()
                                        ),
                                        "namingConvention",
                                    )
                                    .with_help(format!(
                                        "Rename the fragment to use {} casing",
                                        convention.label()
                                    )),
                                );
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

    fn check_with_options(
        source: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
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
        rule.check(&db, file_id, content, metadata, project_files, options)
    }

    fn check(source: &str) -> Vec<LintDiagnostic> {
        check_with_options(source, None)
    }

    #[test]
    fn test_no_options_is_noop() {
        // graphql-eslint parity: with no options every kind is unset, so the
        // rule produces zero diagnostics regardless of how badly named the
        // operation/fragment/variable is.
        let diagnostics = check("query lowercaseOp { user { id } }");
        assert!(diagnostics.is_empty());
        let diagnostics = check("fragment lowercase_frag on User { id }");
        assert!(diagnostics.is_empty());
        let diagnostics = check("query Q($Bad: ID!) { user(id: $Bad) { id } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_valid_operation_name() {
        let opts = serde_json::json!({ "OperationDefinition": "PascalCase" });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_operation_name() {
        let opts = serde_json::json!({ "OperationDefinition": "PascalCase" });
        let diagnostics = check_with_options("query get_user { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PascalCase"));
    }

    #[test]
    fn test_valid_fragment_name() {
        let opts = serde_json::json!({ "FragmentDefinition": "PascalCase" });
        let diagnostics = check_with_options("fragment UserFields on User { id }", Some(&opts));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_fragment_name() {
        let opts = serde_json::json!({ "FragmentDefinition": "PascalCase" });
        let diagnostics = check_with_options("fragment user_fields on User { id }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PascalCase"));
    }

    #[test]
    fn test_valid_variable_name() {
        let opts = serde_json::json!({ "VariableDefinition": "camelCase" });
        let diagnostics = check_with_options(
            "query Q($userId: ID!) { user(id: $userId) { id } }",
            Some(&opts),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_variable_name() {
        let opts = serde_json::json!({ "VariableDefinition": "camelCase" });
        let diagnostics = check_with_options(
            "query Q($UserId: ID!) { user(id: $UserId) { id } }",
            Some(&opts),
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("camelCase"));
    }
}
