mod config;

// New Salsa-based architecture
mod diagnostics;
mod registry;
mod rules;
mod schema_utils;
mod traits;

/// Macro to reduce boilerplate when defining lint rules.
///
/// This macro generates the struct definition and `LintRule` trait implementation
/// for a lint rule, reducing repetitive code across rule implementations.
///
/// # Example
///
/// ```ignore
/// define_lint_rule! {
///     /// Documentation for the rule
///     pub struct MyRuleImpl;
///     name = "my_rule",
///     description = "Description of what this rule checks",
///     severity = Warning,
/// }
/// ```
///
/// This expands to:
///
/// ```ignore
/// /// Documentation for the rule
/// pub struct MyRuleImpl;
///
/// impl LintRule for MyRuleImpl {
///     fn name(&self) -> &'static str { "my_rule" }
///     fn description(&self) -> &'static str { "Description of what this rule checks" }
///     fn default_severity(&self) -> LintSeverity { LintSeverity::Warning }
/// }
/// ```
#[macro_export]
macro_rules! define_lint_rule {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
        name = $rule_name:literal,
        description = $desc:literal,
        severity = $severity:ident $(,)?
    ) => {
        $(#[$meta])*
        $vis struct $name;

        impl $crate::traits::LintRule for $name {
            fn name(&self) -> &'static str {
                $rule_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn default_severity(&self) -> $crate::diagnostics::LintSeverity {
                $crate::diagnostics::LintSeverity::$severity
            }
        }
    };
}

pub use config::{LintConfig, LintRuleConfig, LintSeverity};

// New architecture exports
pub use diagnostics::{
    CodeFix, LintDiagnostic, LintSeverity as DiagnosticSeverity, OffsetRange, TextEdit,
};
pub use registry::{document_schema_rules, project_rules, standalone_document_rules};
pub use traits::{
    DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    StandaloneSchemaLintRule,
};
