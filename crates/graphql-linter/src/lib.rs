mod config;
mod context;
mod linter;

// Old rule implementations (legacy, to be gradually replaced)
pub(crate) mod rules_old;

// New Salsa-based architecture
mod diagnostics;
mod registry;
mod rules;
mod traits;

pub use config::{LintConfig, LintRuleConfig, LintSeverity};
pub use context::{
    DocumentSchemaContext, ProjectContext, StandaloneDocumentContext, StandaloneSchemaContext,
};
pub use linter::Linter;

// New architecture exports
pub use diagnostics::{LintDiagnostic, OffsetRange};
pub use registry::{document_schema_rules, project_rules, standalone_document_rules};
pub use traits::{
    DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    StandaloneSchemaLintRule,
};
