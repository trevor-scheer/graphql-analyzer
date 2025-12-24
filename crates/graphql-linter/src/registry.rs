/// Registry of all available lint rules
///
/// This module provides functions to get all lint rules organized by category.
/// Rules are registered here so that graphql-analysis can query them.
use crate::rules::{
    NoDeprecatedRuleImpl, OperationNameSuffixRuleImpl, RedundantFieldsRuleImpl,
    RequireIdFieldRuleImpl, UniqueNamesRuleImpl, UnusedFieldsRuleImpl, UnusedFragmentsRuleImpl,
};
use crate::traits::{DocumentSchemaLintRule, ProjectLintRule, StandaloneDocumentLintRule};
use std::sync::Arc;

/// Get all standalone document lint rules
///
/// These rules run on individual documents without requiring schema access.
/// They are fast and suitable for real-time LSP diagnostics.
#[must_use]
pub fn standalone_document_rules() -> Vec<Arc<dyn StandaloneDocumentLintRule>> {
    vec![
        Arc::new(RedundantFieldsRuleImpl),
        Arc::new(OperationNameSuffixRuleImpl),
    ]
}

/// Get all document+schema lint rules
///
/// These rules run on individual documents with schema access.
/// They are suitable for real-time LSP diagnostics.
#[must_use]
pub fn document_schema_rules() -> Vec<Arc<dyn DocumentSchemaLintRule>> {
    vec![
        Arc::new(NoDeprecatedRuleImpl),
        Arc::new(RequireIdFieldRuleImpl),
    ]
}

/// Get all project-wide lint rules
///
/// These rules analyze the entire project and are expensive.
/// They should only run in CLI/CI, not in real-time LSP.
#[must_use]
pub fn project_rules() -> Vec<Arc<dyn ProjectLintRule>> {
    vec![
        Arc::new(UniqueNamesRuleImpl),
        Arc::new(UnusedFieldsRuleImpl),
        Arc::new(UnusedFragmentsRuleImpl),
    ]
}
