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

    /// Convert `snake_case` to `camelCase` (e.g., `no_deprecated` -> `noDeprecated`)
    fn snake_to_camel(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut capitalize_next = false;
        for c in s.chars() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Test that all lint rules are documented in the JSON schema for autocomplete.
    ///
    /// When you add a new lint rule, you must also add it to the schema at:
    /// `crates/config/schema/graphqlrc.schema.json` under
    /// `definitions.FullLintConfig.properties.rules.properties`
    #[test]
    fn test_schema_includes_all_rules() {
        // Load the JSON schema
        let schema_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/schema/graphqlrc.schema.json"
        );
        let schema_str = std::fs::read_to_string(schema_path).expect("Failed to read schema file");
        let schema: serde_json::Value =
            serde_json::from_str(&schema_str).expect("Failed to parse schema JSON");

        // Extract rule names from schema (camelCase)
        let schema_rules: std::collections::HashSet<String> = schema
            .get("definitions")
            .and_then(|d| d.get("FullLintConfig"))
            .and_then(|f| f.get("properties"))
            .and_then(|p| p.get("rules"))
            .and_then(|r| r.get("properties"))
            .and_then(|p| p.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        // Get all rule names from registry (snake_case) and convert to camelCase
        let registry_rules: std::collections::HashSet<String> =
            all_rule_names().into_iter().map(snake_to_camel).collect();

        // Find rules missing from schema
        let missing_from_schema: Vec<_> = registry_rules.difference(&schema_rules).collect();

        // Find rules in schema that don't exist in registry (stale entries)
        let stale_in_schema: Vec<_> = schema_rules.difference(&registry_rules).collect();

        assert!(
            missing_from_schema.is_empty() && stale_in_schema.is_empty(),
            "JSON schema is out of sync with lint rule registry!\n\n\
             Missing from schema (add these to graphqlrc.schema.json):\n  {}\n\n\
             Stale in schema (remove these from graphqlrc.schema.json):\n  {}\n\n\
             Schema location: crates/config/schema/graphqlrc.schema.json\n\
             Path: definitions.FullLintConfig.properties.rules.properties",
            if missing_from_schema.is_empty() {
                "(none)".to_string()
            } else {
                missing_from_schema
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            if stale_in_schema.is_empty() {
                "(none)".to_string()
            } else {
                stale_in_schema
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
    }
}
