/// Registry of all available lint rules
use crate::rules::{
    NoAnonymousOperationsRuleImpl, NoDeprecatedRuleImpl, OperationNameSuffixRuleImpl,
    RedundantFieldsRuleImpl, RequireIdFieldRuleImpl, UniqueNamesRuleImpl, UnusedFieldsRuleImpl,
    UnusedFragmentsRuleImpl, UnusedVariablesRuleImpl,
};
use crate::traits::{DocumentSchemaLintRule, ProjectLintRule, StandaloneDocumentLintRule};
use std::sync::{Arc, LazyLock};

/// Lazily initialized standalone document rules.
/// Rules are created once and reused across all calls.
static STANDALONE_DOCUMENT_RULES: LazyLock<Vec<Arc<dyn StandaloneDocumentLintRule>>> =
    LazyLock::new(|| {
        vec![
            Arc::new(NoAnonymousOperationsRuleImpl),
            Arc::new(OperationNameSuffixRuleImpl),
            Arc::new(RedundantFieldsRuleImpl),
            Arc::new(UnusedVariablesRuleImpl),
        ]
    });

/// Lazily initialized document-schema rules.
/// Rules are created once and reused across all calls.
static DOCUMENT_SCHEMA_RULES: LazyLock<Vec<Arc<dyn DocumentSchemaLintRule>>> =
    LazyLock::new(|| {
        vec![
            Arc::new(NoDeprecatedRuleImpl),
            Arc::new(RequireIdFieldRuleImpl),
        ]
    });

/// Lazily initialized project rules.
/// Rules are created once and reused across all calls.
static PROJECT_RULES: LazyLock<Vec<Arc<dyn ProjectLintRule>>> = LazyLock::new(|| {
    vec![
        Arc::new(UniqueNamesRuleImpl),
        Arc::new(UnusedFieldsRuleImpl),
        Arc::new(UnusedFragmentsRuleImpl),
    ]
});

#[must_use]
pub fn standalone_document_rules() -> &'static [Arc<dyn StandaloneDocumentLintRule>] {
    &STANDALONE_DOCUMENT_RULES
}

#[must_use]
pub fn document_schema_rules() -> &'static [Arc<dyn DocumentSchemaLintRule>] {
    &DOCUMENT_SCHEMA_RULES
}

#[must_use]
pub fn project_rules() -> &'static [Arc<dyn ProjectLintRule>] {
    &PROJECT_RULES
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
