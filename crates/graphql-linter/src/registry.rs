/// Registry of all available lint rules
use crate::rules::{
    NoDeprecatedRuleImpl, OperationNameSuffixRuleImpl, RedundantFieldsRuleImpl,
    RequireIdFieldRuleImpl, UniqueNamesRuleImpl, UnusedFieldsRuleImpl, UnusedFragmentsRuleImpl,
    UnusedVariablesRuleImpl,
};
use crate::traits::{DocumentSchemaLintRule, ProjectLintRule, StandaloneDocumentLintRule};
use std::sync::Arc;

#[must_use]
pub fn standalone_document_rules() -> Vec<Arc<dyn StandaloneDocumentLintRule>> {
    vec![
        Arc::new(OperationNameSuffixRuleImpl),
        Arc::new(RedundantFieldsRuleImpl),
        Arc::new(UnusedVariablesRuleImpl),
    ]
}

#[must_use]
pub fn document_schema_rules() -> Vec<Arc<dyn DocumentSchemaLintRule>> {
    vec![
        Arc::new(NoDeprecatedRuleImpl),
        Arc::new(RequireIdFieldRuleImpl),
    ]
}

#[must_use]
pub fn project_rules() -> Vec<Arc<dyn ProjectLintRule>> {
    vec![
        Arc::new(UniqueNamesRuleImpl),
        Arc::new(UnusedFieldsRuleImpl),
        Arc::new(UnusedFragmentsRuleImpl),
    ]
}

#[must_use]
pub fn all_rule_names() -> Vec<&'static str> {
    let mut names = Vec::new();

    for rule in standalone_document_rules() {
        names.push(rule.name());
    }
    for rule in document_schema_rules() {
        names.push(rule.name());
    }
    for rule in project_rules() {
        names.push(rule.name());
    }

    names.sort_unstable();
    names
}
