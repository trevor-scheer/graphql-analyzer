use crate::{Diagnostic, DocumentIndex, SchemaIndex, Severity};

use super::config::{LintConfig, LintSeverity};
use super::rules;

/// Linter that runs configured project-wide lint rules
///
/// Note: Per-document lint rules (like `deprecated_field`) have been moved to graphql-linter.
/// This linter only handles project-wide rules (`unique_names`, `unused_fields`) that need
/// access to the entire document index.
pub struct Linter {
    config: LintConfig,
}

impl Linter {
    /// Create a new linter with the given configuration
    #[must_use]
    pub const fn new(config: LintConfig) -> Self {
        Self { config }
    }

    /// Run all enabled project-wide lints across all documents
    #[must_use]
    pub fn lint_project(
        &self,
        document_index: &DocumentIndex,
        schema_index: &SchemaIndex,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Get all available project-wide rules
        let all_project_rules = rules::all_project_rules();

        for rule in all_project_rules {
            let rule_name = rule.name();

            // Skip if rule is not enabled (opt-in behavior)
            if !self.config.is_enabled(rule_name) {
                continue;
            }

            // Run the rule
            let mut rule_diagnostics = rule.check_project(document_index, schema_index);

            // Apply configured severity
            if let Some(severity) = self.config.get_severity(rule_name) {
                for diag in &mut rule_diagnostics {
                    diag.severity = match severity {
                        LintSeverity::Error => Severity::Error,
                        LintSeverity::Warn => Severity::Warning,
                        LintSeverity::Off => unreachable!("Off rules are skipped"),
                    };
                }
            }

            diagnostics.extend(rule_diagnostics);
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schema() -> SchemaIndex {
        SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String
            }
        ",
        )
    }

    #[test]
    fn test_linter_with_no_config_runs_no_lints() {
        let config = LintConfig::default();
        let linter = Linter::new(config);
        let schema = create_test_schema();
        let document_index = DocumentIndex::new();

        let diagnostics = linter.lint_project(&document_index, &schema);
        assert_eq!(
            diagnostics.len(),
            0,
            "No diagnostics should be generated without config"
        );
    }
}
