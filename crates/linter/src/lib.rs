mod config;

// New Salsa-based architecture
mod diagnostics;
pub mod ignore;
mod registry;
mod rules;
mod schema_utils;
mod traits;

pub use config::{LintConfig, LintRuleConfig, LintSeverity};

// New architecture exports
pub use diagnostics::{
    CodeFix, LintDiagnostic, LintSeverity as DiagnosticSeverity, OffsetRange, TextEdit,
};
pub use graphql_syntax::SourceSpan;
pub use registry::{
    all_rule_info, all_rule_names, document_schema_rules, project_rules, standalone_document_rules,
    standalone_schema_rules, RuleCategory, RuleInfo,
};
pub use traits::{
    DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    StandaloneSchemaLintRule,
};
