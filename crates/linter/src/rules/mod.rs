/// Lint rule implementations using the new Salsa-based architecture
///
/// Each rule is implemented in its own file and implements one of the trait types:
/// - `StandaloneDocumentLintRule` - Rules that don't need schema access
/// - `DocumentSchemaLintRule` - Rules that need schema access
/// - `ProjectLintRule` - Rules that analyze the entire project
use apollo_parser::cst;

/// The kind of GraphQL operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

/// Get the operation kind from an operation type node
pub fn get_operation_kind(op_type: &cst::OperationType) -> OperationKind {
    if op_type.query_token().is_some() {
        OperationKind::Query
    } else if op_type.mutation_token().is_some() {
        OperationKind::Mutation
    } else {
        OperationKind::Subscription
    }
}

mod no_anonymous_operations;
mod no_deprecated;
mod operation_name_suffix;
mod redundant_fields;
mod require_id_field;
mod unique_names;
mod unused_fields;
mod unused_fragments;
mod unused_variables;

pub use no_anonymous_operations::NoAnonymousOperationsRuleImpl;
pub use no_deprecated::NoDeprecatedRuleImpl;
pub use operation_name_suffix::OperationNameSuffixRuleImpl;
pub use redundant_fields::RedundantFieldsRuleImpl;
pub use require_id_field::RequireIdFieldRuleImpl;
pub use unique_names::UniqueNamesRuleImpl;
pub use unused_fields::UnusedFieldsRuleImpl;
pub use unused_fragments::UnusedFragmentsRuleImpl;
pub use unused_variables::UnusedVariablesRuleImpl;
