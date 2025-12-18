use crate::config::{LintConfig, LintSeverity};
use crate::context::{
    DocumentSchemaContext, ProjectContext, StandaloneDocumentContext, StandaloneSchemaContext,
};
use crate::rules;
use graphql_project::{Diagnostic, Severity};
use std::collections::HashMap;

/// Linter that runs configured lint rules
pub struct Linter {
    config: LintConfig,
}

impl Linter {
    /// Create a new linter with the given configuration
    #[must_use]
    pub const fn new(config: LintConfig) -> Self {
        Self { config }
    }

    /// Lint a standalone document (no schema)
    /// Currently no rules exist for this scenario
    #[must_use]
    #[tracing::instrument(skip(self, document, fragments), fields(file = file_name))]
    pub fn lint_standalone_document(
        &self,
        document: &str,
        file_name: &str,
        fragments: Option<&graphql_project::DocumentIndex>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Parse the document once
        let parsed = apollo_parser::Parser::new(document).parse();

        // If there are parse errors, return early
        if parsed.errors().len() > 0 {
            tracing::debug!("Document has parse errors, skipping linting");
            return diagnostics;
        }

        // Create context with pre-parsed tree
        let ctx = StandaloneDocumentContext {
            document,
            file_name,
            fragments,
            parsed: &parsed,
        };

        // Get all available standalone document rules
        let all_rules = rules::all_standalone_document_rules();
        tracing::debug!(
            rules_count = all_rules.len(),
            "Running standalone document rules"
        );

        for rule in all_rules {
            let rule_name = rule.name();

            // Skip if rule is not enabled (opt-in behavior)
            if !self.config.is_enabled(rule_name) {
                tracing::trace!(rule = rule_name, "Rule not enabled, skipping");
                continue;
            }

            tracing::trace!(rule = rule_name, "Running rule");
            // Run the rule
            let mut rule_diagnostics = rule.check(&ctx);

            // Apply configured severity
            if let Some(severity) = self.config.get_severity(rule_name) {
                apply_severity(&mut rule_diagnostics, severity);
            }

            if !rule_diagnostics.is_empty() {
                tracing::debug!(
                    rule = rule_name,
                    diagnostics = rule_diagnostics.len(),
                    "Rule found issues"
                );
            }

            diagnostics.extend(rule_diagnostics);
        }

        tracing::debug!(
            total_diagnostics = diagnostics.len(),
            "Standalone document linting complete"
        );
        diagnostics
    }

    /// Lint a document against a schema
    #[must_use]
    #[tracing::instrument(skip(self, document, schema), fields(file = file_name))]
    pub fn lint_document(
        &self,
        document: &str,
        file_name: &str,
        schema: &graphql_project::SchemaIndex,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Parse the document once
        let parsed = apollo_parser::Parser::new(document).parse();

        // If there are parse errors, return early
        if parsed.errors().len() > 0 {
            tracing::debug!("Document has parse errors, skipping linting");
            return diagnostics;
        }

        // Create context with pre-parsed tree
        let ctx = DocumentSchemaContext {
            document,
            file_name,
            schema,
            parsed: &parsed,
        };

        // Get all available document+schema rules
        let all_rules = rules::all_document_schema_rules();
        tracing::debug!(
            rules_count = all_rules.len(),
            "Running document+schema rules"
        );

        for rule in all_rules {
            let rule_name = rule.name();

            // Skip if rule is not enabled (opt-in behavior)
            if !self.config.is_enabled(rule_name) {
                tracing::trace!(rule = rule_name, "Rule not enabled, skipping");
                continue;
            }

            tracing::trace!(rule = rule_name, "Running rule");
            // Run the rule
            let mut rule_diagnostics = rule.check(&ctx);

            // Apply configured severity
            if let Some(severity) = self.config.get_severity(rule_name) {
                apply_severity(&mut rule_diagnostics, severity);
            }

            if !rule_diagnostics.is_empty() {
                tracing::debug!(
                    rule = rule_name,
                    diagnostics = rule_diagnostics.len(),
                    "Rule found issues"
                );
            }

            diagnostics.extend(rule_diagnostics);
        }

        tracing::debug!(
            total_diagnostics = diagnostics.len(),
            "Document linting complete"
        );
        diagnostics
    }

    /// Lint a standalone schema
    /// Currently no rules exist for this scenario
    #[must_use]
    pub fn lint_schema(&self, ctx: &StandaloneSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Get all available standalone schema rules
        let all_rules = rules::all_standalone_schema_rules();

        for rule in all_rules {
            let rule_name = rule.name();

            // Skip if rule is not enabled (opt-in behavior)
            if !self.config.is_enabled(rule_name) {
                continue;
            }

            // Run the rule
            let mut rule_diagnostics = rule.check(ctx);

            // Apply configured severity
            if let Some(severity) = self.config.get_severity(rule_name) {
                apply_severity(&mut rule_diagnostics, severity);
            }

            diagnostics.extend(rule_diagnostics);
        }

        diagnostics
    }

    /// Lint an entire project (expensive, project-wide)
    /// Returns diagnostics grouped by file path
    #[must_use]
    #[tracing::instrument(skip(self, ctx))]
    pub fn lint_project(&self, ctx: &ProjectContext) -> HashMap<String, Vec<Diagnostic>> {
        let mut diagnostics_by_file: HashMap<String, Vec<Diagnostic>> = HashMap::new();

        // Get all available project-wide rules
        let all_project_rules = rules::all_project_rules();
        tracing::info!(
            rules_count = all_project_rules.len(),
            "Running project-wide lint rules"
        );

        for rule in all_project_rules {
            let rule_name = rule.name();

            // Skip if rule is not enabled (opt-in behavior)
            if !self.config.is_enabled(rule_name) {
                tracing::debug!(rule = rule_name, "Project rule not enabled, skipping");
                continue;
            }

            tracing::info!(rule = rule_name, "Running project-wide rule");
            // Run the rule
            let rule_diagnostics = rule.check(ctx);

            let files_with_issues = rule_diagnostics.len();
            let total_issues: usize = rule_diagnostics.values().map(Vec::len).sum();

            if total_issues > 0 {
                tracing::info!(
                    rule = rule_name,
                    files = files_with_issues,
                    issues = total_issues,
                    "Project rule found issues"
                );
            }

            // Apply configured severity and merge into result
            if let Some(severity) = self.config.get_severity(rule_name) {
                for (file_path, mut file_diagnostics) in rule_diagnostics {
                    apply_severity(&mut file_diagnostics, severity);
                    diagnostics_by_file
                        .entry(file_path)
                        .or_default()
                        .extend(file_diagnostics);
                }
            }
        }

        let total_files_with_issues = diagnostics_by_file.len();
        let total_issues: usize = diagnostics_by_file.values().map(Vec::len).sum();
        tracing::info!(
            files = total_files_with_issues,
            issues = total_issues,
            "Project linting complete"
        );

        diagnostics_by_file
    }
}

/// Apply severity level to diagnostics
fn apply_severity(diagnostics: &mut [Diagnostic], severity: LintSeverity) {
    for diag in diagnostics {
        diag.severity = match severity {
            LintSeverity::Error => Severity::Error,
            LintSeverity::Warn => Severity::Warning,
            LintSeverity::Off => unreachable!("Off rules are skipped"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_project::SchemaIndex;

    fn create_test_schema() -> SchemaIndex {
        SchemaIndex::from_schema(
            r#"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String @deprecated(reason: "Use 'emailAddress' instead")
                emailAddress: String
            }
        "#,
        )
    }

    #[test]
    fn test_linter_with_no_config_runs_no_lints() {
        let config = LintConfig::default();
        let linter = Linter::new(config);
        let schema = create_test_schema();

        let document = r#"
            query GetUser { user(id: "1") { id } }
            query GetUser { user(id: "2") { name } }
        "#;

        let diagnostics = linter.lint_document(document, "test.graphql", &schema);
        assert_eq!(
            diagnostics.len(),
            0,
            "No diagnostics should be generated without config"
        );
    }

    #[test]
    fn test_linter_with_recommended_config() {
        let config = LintConfig::recommended();
        let linter = Linter::new(config);
        let schema = create_test_schema();

        let document = r#"
            query GetUser { user(id: "1") { id email } }
        "#;

        let diagnostics = linter.lint_document(document, "test.graphql", &schema);

        // Should have 1 warning for deprecated field
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();
        assert_eq!(
            warning_count, 1,
            "Should have 1 warning for deprecated field"
        );
    }

    #[test]
    fn test_linter_respects_custom_severity() {
        let yaml = "\ndeprecated_field: error\n";
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        let linter = Linter::new(config);
        let schema = create_test_schema();

        let document = r#"
            query GetUser { user(id: "1") { id email } }
        "#;

        let diagnostics = linter.lint_document(document, "test.graphql", &schema);

        // Deprecated field should be error (custom config)
        let deprecated_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("deprecated"))
            .collect();
        assert_eq!(
            deprecated_diags.len(),
            1,
            "Should have one deprecated warning"
        );
        assert!(deprecated_diags
            .iter()
            .all(|d| d.severity == Severity::Error));
    }

    #[test]
    fn test_linter_can_disable_specific_rules() {
        let yaml = "\ndeprecated_field: off\n";
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        let linter = Linter::new(config);
        let schema = create_test_schema();

        let document = r#"
            query GetUser { user(id: "1") { id email } }
        "#;

        let diagnostics = linter.lint_document(document, "test.graphql", &schema);

        // Should have no diagnostics since deprecated_field is disabled
        assert_eq!(
            diagnostics.len(),
            0,
            "Should have no diagnostics when rule is disabled"
        );
    }
}
