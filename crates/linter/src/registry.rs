/// Registry of all available lint rules
use crate::rules::{
    AlphabetizeRuleImpl, DescriptionStyleRuleImpl, InputNameRuleImpl,
    LoneExecutableDefinitionRuleImpl, NamingConventionRuleImpl, NoAnonymousOperationsRuleImpl,
    NoDeprecatedRuleImpl, NoDuplicateFieldsRuleImpl, NoHashtagDescriptionRuleImpl,
    NoOnePlaceFragmentsRuleImpl, NoScalarResultTypeOnMutationRuleImpl, NoTypenamePrefixRuleImpl,
    NoUnreachableTypesRuleImpl, OperationNameSuffixRuleImpl, RedundantFieldsRuleImpl,
    RequireDeprecationReasonRuleImpl, RequireDescriptionRuleImpl,
    RequireFieldOfTypeQueryInMutationResultRuleImpl, RequireIdFieldRuleImpl,
    SelectionSetDepthRuleImpl, StrictIdInTypesRuleImpl, UniqueEnumValueNamesRuleImpl,
    UniqueNamesRuleImpl, UnusedFieldsRuleImpl, UnusedFragmentsRuleImpl, UnusedVariablesRuleImpl,
};
use crate::traits::{
    DocumentSchemaLintRule, ProjectLintRule, StandaloneDocumentLintRule, StandaloneSchemaLintRule,
};
use std::sync::{Arc, LazyLock};

/// Lazily initialized standalone document rules.
/// Rules are created once and reused across all calls.
static STANDALONE_DOCUMENT_RULES: LazyLock<Vec<Arc<dyn StandaloneDocumentLintRule>>> =
    LazyLock::new(|| {
        vec![
            Arc::new(AlphabetizeRuleImpl),
            Arc::new(LoneExecutableDefinitionRuleImpl),
            Arc::new(NamingConventionRuleImpl),
            Arc::new(NoAnonymousOperationsRuleImpl),
            Arc::new(NoDuplicateFieldsRuleImpl),
            Arc::new(OperationNameSuffixRuleImpl),
            Arc::new(RedundantFieldsRuleImpl),
            Arc::new(SelectionSetDepthRuleImpl),
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
        Arc::new(NoOnePlaceFragmentsRuleImpl),
        Arc::new(UniqueNamesRuleImpl),
        Arc::new(UnusedFieldsRuleImpl),
        Arc::new(UnusedFragmentsRuleImpl),
    ]
});

/// Lazily initialized standalone schema rules.
/// Rules are created once and reused across all calls.
static STANDALONE_SCHEMA_RULES: LazyLock<Vec<Arc<dyn StandaloneSchemaLintRule>>> =
    LazyLock::new(|| {
        vec![
            Arc::new(DescriptionStyleRuleImpl),
            Arc::new(InputNameRuleImpl),
            Arc::new(NoHashtagDescriptionRuleImpl),
            Arc::new(NoScalarResultTypeOnMutationRuleImpl),
            Arc::new(NoTypenamePrefixRuleImpl),
            Arc::new(NoUnreachableTypesRuleImpl),
            Arc::new(RequireDeprecationReasonRuleImpl),
            Arc::new(RequireDescriptionRuleImpl),
            Arc::new(RequireFieldOfTypeQueryInMutationResultRuleImpl),
            Arc::new(StrictIdInTypesRuleImpl),
            Arc::new(UniqueEnumValueNamesRuleImpl),
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
pub fn standalone_schema_rules() -> &'static [Arc<dyn StandaloneSchemaLintRule>] {
    &STANDALONE_SCHEMA_RULES
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
    for rule in standalone_schema_rules() {
        names.push(rule.name());
    }

    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standalone_document_rules_not_empty() {
        let rules = standalone_document_rules();
        assert!(!rules.is_empty());
    }

    #[test]
    fn test_document_schema_rules_not_empty() {
        let rules = document_schema_rules();
        assert!(!rules.is_empty());
    }

    #[test]
    fn test_project_rules_not_empty() {
        let rules = project_rules();
        assert!(!rules.is_empty());
    }

    #[test]
    fn test_standalone_schema_rules_not_empty() {
        let rules = standalone_schema_rules();
        assert!(!rules.is_empty());
    }

    #[test]
    fn test_all_rule_names_returns_sorted_list() {
        let names = all_rule_names();
        assert!(!names.is_empty());

        let mut sorted_names = names.clone();
        sorted_names.sort_unstable();
        assert_eq!(names, sorted_names);
    }

    #[test]
    fn test_all_rule_names_includes_expected_rules() {
        let names = all_rule_names();
        assert!(names.contains(&"no_anonymous_operations"));
        assert!(names.contains(&"no_deprecated"));
        assert!(names.contains(&"unique_names"));
        assert!(names.contains(&"unused_fragments"));
        // New rules
        assert!(names.contains(&"no_duplicate_fields"));
        assert!(names.contains(&"selection_set_depth"));
        assert!(names.contains(&"naming_convention"));
        assert!(names.contains(&"require_description"));
        assert!(names.contains(&"no_unreachable_types"));
    }

    #[test]
    fn test_rules_have_unique_names() {
        let names = all_rule_names();
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(seen.insert(*name), "Duplicate rule name: {name}");
        }
    }

    #[test]
    fn test_standalone_rules_have_valid_metadata() {
        for rule in standalone_document_rules() {
            assert!(!rule.name().is_empty());
            assert!(!rule.description().is_empty());
        }
    }

    #[test]
    fn test_document_schema_rules_have_valid_metadata() {
        for rule in document_schema_rules() {
            assert!(!rule.name().is_empty());
            assert!(!rule.description().is_empty());
        }
    }

    #[test]
    fn test_project_rules_have_valid_metadata() {
        for rule in project_rules() {
            assert!(!rule.name().is_empty());
            assert!(!rule.description().is_empty());
        }
    }

    #[test]
    fn test_standalone_schema_rules_have_valid_metadata() {
        for rule in standalone_schema_rules() {
            assert!(!rule.name().is_empty());
            assert!(!rule.description().is_empty());
        }
    }
}
