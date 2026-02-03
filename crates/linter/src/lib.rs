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
pub use registry::{document_schema_rules, project_rules, standalone_document_rules};
pub use traits::{
    DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    StandaloneSchemaLintRule,
};

/// Prelude module for convenient imports.
///
/// This module re-exports the most commonly used types for working with
/// the linter. Import with:
///
/// ```rust,ignore
/// use graphql_linter::prelude::*;
/// ```
pub mod prelude {
    pub use crate::config::{LintConfig, LintSeverity};
    pub use crate::diagnostics::{LintDiagnostic, LintSeverity as DiagnosticSeverity, OffsetRange};
    pub use crate::traits::{
        DocumentSchemaLintRule, LintRule, ProjectLintRule, StandaloneDocumentLintRule,
    };
}
