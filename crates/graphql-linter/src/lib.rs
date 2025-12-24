mod config;
mod context;
mod linter;
mod rules;

// New Salsa-based architecture
mod diagnostics;
mod traits;

pub use config::{LintConfig, LintRuleConfig, LintSeverity};
pub use context::{
    DocumentSchemaContext, ProjectContext, StandaloneDocumentContext, StandaloneSchemaContext,
};
pub use linter::Linter;

// New architecture exports
pub use diagnostics::{LintDiagnostic, OffsetRange};
pub use traits::{
    DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    StandaloneSchemaLintRule,
};
