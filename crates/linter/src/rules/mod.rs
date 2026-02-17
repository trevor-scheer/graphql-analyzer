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

mod alphabetize;
mod description_style;
mod input_name;
mod lone_executable_definition;
mod naming_convention;
mod no_anonymous_operations;
mod no_deprecated;
mod no_duplicate_fields;
mod no_hashtag_description;
mod no_one_place_fragments;
mod no_scalar_result_type_on_mutation;
mod no_typename_prefix;
mod no_unreachable_types;
mod operation_name_suffix;
mod redundant_fields;
mod require_deprecation_reason;
mod require_description;
mod require_field_of_type_query_in_mutation_result;
mod require_id_field;
mod selection_set_depth;
mod strict_id_in_types;
mod unique_enum_value_names;
mod unique_names;
mod unused_fields;
mod unused_fragments;
mod unused_variables;

pub use alphabetize::AlphabetizeRuleImpl;
pub use description_style::DescriptionStyleRuleImpl;
pub use input_name::InputNameRuleImpl;
pub use lone_executable_definition::LoneExecutableDefinitionRuleImpl;
pub use naming_convention::NamingConventionRuleImpl;
pub use no_anonymous_operations::NoAnonymousOperationsRuleImpl;
pub use no_deprecated::NoDeprecatedRuleImpl;
pub use no_duplicate_fields::NoDuplicateFieldsRuleImpl;
pub use no_hashtag_description::NoHashtagDescriptionRuleImpl;
pub use no_one_place_fragments::NoOnePlaceFragmentsRuleImpl;
pub use no_scalar_result_type_on_mutation::NoScalarResultTypeOnMutationRuleImpl;
pub use no_typename_prefix::NoTypenamePrefixRuleImpl;
pub use no_unreachable_types::NoUnreachableTypesRuleImpl;
pub use operation_name_suffix::OperationNameSuffixRuleImpl;
pub use redundant_fields::RedundantFieldsRuleImpl;
pub use require_deprecation_reason::RequireDeprecationReasonRuleImpl;
pub use require_description::RequireDescriptionRuleImpl;
pub use require_field_of_type_query_in_mutation_result::RequireFieldOfTypeQueryInMutationResultRuleImpl;
pub use require_id_field::RequireIdFieldRuleImpl;
pub use selection_set_depth::SelectionSetDepthRuleImpl;
pub use strict_id_in_types::StrictIdInTypesRuleImpl;
pub use unique_enum_value_names::UniqueEnumValueNamesRuleImpl;
pub use unique_names::UniqueNamesRuleImpl;
pub use unused_fields::UnusedFieldsRuleImpl;
pub use unused_fragments::UnusedFragmentsRuleImpl;
pub use unused_variables::UnusedVariablesRuleImpl;
