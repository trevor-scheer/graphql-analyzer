/// Lint rule implementations using the new Salsa-based architecture
///
/// Each rule is implemented in its own file and implements one of the trait types:
/// - `StandaloneDocumentLintRule` - Rules that don't need schema access
/// - `DocumentSchemaLintRule` - Rules that need schema access
/// - `ProjectLintRule` - Rules that analyze the entire project
mod deprecated_field;
mod operation_name_suffix;
mod redundant_fields;
mod require_id_field;
mod unique_names;
mod unused_fields;
mod unused_fragments;

pub use deprecated_field::DeprecatedFieldRuleImpl;
pub use operation_name_suffix::OperationNameSuffixRuleImpl;
pub use redundant_fields::RedundantFieldsRuleImpl;
pub use require_id_field::RequireIdFieldRuleImpl;
pub use unique_names::UniqueNamesRuleImpl;
pub use unused_fields::UnusedFieldsRuleImpl;
pub use unused_fragments::UnusedFragmentsRuleImpl;

// Helper to convert graphql_project::Diagnostic to LintDiagnostic
use crate::diagnostics::{LintDiagnostic, LintSeverity, OffsetRange};

pub fn convert_diagnostic(diag: &graphql_project::Diagnostic) -> LintDiagnostic {
    // Convert line/column back to byte offsets
    // This is a temporary bridge - eventually rules will emit byte offsets directly
    let start_offset = diag.range.start.line * 1000 + diag.range.start.character;
    let end_offset = diag.range.end.line * 1000 + diag.range.end.character;

    LintDiagnostic {
        offset_range: OffsetRange::new(start_offset, end_offset),
        severity: match diag.severity {
            graphql_project::Severity::Error => LintSeverity::Error,
            graphql_project::Severity::Warning => LintSeverity::Warning,
            graphql_project::Severity::Information | graphql_project::Severity::Hint => {
                LintSeverity::Info
            }
        },
        message: diag.message.clone(),
        rule: diag
            .code
            .as_ref()
            .map_or_else(|| "unknown".to_string(), std::clone::Clone::clone),
    }
}
