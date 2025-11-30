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
    pub fn lint_standalone_document(&self, ctx: &StandaloneDocumentContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Get all available standalone document rules
        let all_rules = rules::all_standalone_document_rules();

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

    /// Lint a document against a schema
    #[must_use]
    pub fn lint_document(&self, ctx: &DocumentSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Get all available document+schema rules
        let all_rules = rules::all_document_schema_rules();

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
    pub fn lint_project(&self, ctx: &ProjectContext) -> HashMap<String, Vec<Diagnostic>> {
        let mut diagnostics_by_file: HashMap<String, Vec<Diagnostic>> = HashMap::new();

        // Get all available project-wide rules
        let all_project_rules = rules::all_project_rules();

        for rule in all_project_rules {
            let rule_name = rule.name();

            // Skip if rule is not enabled (opt-in behavior)
            if !self.config.is_enabled(rule_name) {
                continue;
            }

            // Run the rule
            let rule_diagnostics = rule.check(ctx);

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

        let ctx = DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
        };

        let diagnostics = linter.lint_document(&ctx);
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

        let ctx = DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
        };

        let diagnostics = linter.lint_document(&ctx);

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

        let ctx = DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
        };

        let diagnostics = linter.lint_document(&ctx);

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

        let ctx = DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
        };

        let diagnostics = linter.lint_document(&ctx);

        // Should have no diagnostics since deprecated_field is disabled
        assert_eq!(
            diagnostics.len(),
            0,
            "Should have no diagnostics when rule is disabled"
        );
    }
}
