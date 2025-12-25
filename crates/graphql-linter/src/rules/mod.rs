/// Lint rule implementations using the new Salsa-based architecture
///
/// Each rule is implemented in its own file and implements one of the trait types:
/// - `StandaloneDocumentLintRule` - Rules that don't need schema access
/// - `DocumentSchemaLintRule` - Rules that need schema access
/// - `ProjectLintRule` - Rules that analyze the entire project
mod no_deprecated;
mod operation_name_suffix;
mod require_id_field;
mod unique_names;
mod unused_fields;
mod unused_fragments;

pub use no_deprecated::NoDeprecatedRuleImpl;
pub use operation_name_suffix::OperationNameSuffixRuleImpl;
pub use require_id_field::RequireIdFieldRuleImpl;
pub use unique_names::UniqueNamesRuleImpl;
pub use unused_fields::UnusedFieldsRuleImpl;
pub use unused_fragments::UnusedFragmentsRuleImpl;
