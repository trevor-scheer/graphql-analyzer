mod config;

// New Salsa-based architecture
mod diagnostics;
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
pub use registry::{document_schema_rules, project_rules, standalone_document_rules};
pub use traits::{
    DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    StandaloneSchemaLintRule,
};
