use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity level for a lint rule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Off,
    Warn,
    Error,
}

/// Configuration for a single lint rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LintRuleConfig {
    /// Just a severity level (simple case)
    Severity(LintSeverity),

    /// Detailed config with options (future)
    Detailed {
        severity: LintSeverity,
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<serde_json::Value>,
    },
}

/// Overall lint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LintConfig {
    /// Use recommended preset
    Recommended(String), // "recommended"

    /// Custom rule configuration
    Rules {
        #[serde(flatten)]
        rules: HashMap<String, LintRuleConfig>,
    },
}

impl Default for LintConfig {
    fn default() -> Self {
        // Default is no lints enabled (opt-in)
        Self::Rules {
            rules: HashMap::new(),
        }
    }
}

impl LintConfig {
    /// Get the severity for a rule, considering recommended preset
    #[must_use]
    pub fn get_severity(&self, rule_name: &str) -> Option<LintSeverity> {
        match self {
            Self::Recommended(_) => Self::recommended_severity(rule_name),
            Self::Rules { rules } => {
                // Check if "recommended" is set
                if matches!(
                    rules.get("recommended"),
                    Some(LintRuleConfig::Severity(
                        LintSeverity::Warn | LintSeverity::Error
                    ))
                ) {
                    // Start with recommended, allow overrides
                    let recommended = Self::recommended_severity(rule_name);
                    rules
                        .get(rule_name)
                        .map(|config| match config {
                            LintRuleConfig::Severity(severity)
                            | LintRuleConfig::Detailed { severity, .. } => *severity,
                        })
                        .or(recommended)
                } else {
                    // No recommended, only explicit rules
                    rules.get(rule_name).map(|config| match config {
                        LintRuleConfig::Severity(severity)
                        | LintRuleConfig::Detailed { severity, .. } => *severity,
                    })
                }
            }
        }
    }

    /// Check if a rule is enabled (not Off and not None)
    #[must_use]
    pub fn is_enabled(&self, rule_name: &str) -> bool {
        matches!(
            self.get_severity(rule_name),
            Some(LintSeverity::Warn | LintSeverity::Error)
        )
    }

    /// Get recommended severity for a rule
    fn recommended_severity(rule_name: &str) -> Option<LintSeverity> {
        match rule_name {
            "unique_names" | "no_anonymous_operations" => Some(LintSeverity::Error),
            "deprecated_field"
            | "field_names_should_be_camel_case"
            | "type_names_should_be_pascal_case"
            | "enum_values_should_be_screaming_snake_case" => Some(LintSeverity::Warn),
            _ => None,
        }
    }

    /// Get recommended configuration
    #[must_use]
    pub fn recommended() -> Self {
        Self::Recommended("recommended".to_string())
    }

    /// Merge another config into this one (tool-specific overrides)
    #[must_use]
    pub fn merge(&self, override_config: &Self) -> Self {
        match (self, override_config) {
            // If override is empty, just use base
            (base, Self::Rules { rules }) if rules.is_empty() => base.clone(),

            // If override is Recommended, use it
            (_, Self::Recommended(s)) => Self::Recommended(s.clone()),

            // Merge rules
            (
                Self::Rules { rules: base_rules },
                Self::Rules {
                    rules: override_rules,
                },
            ) => {
                let mut merged = base_rules.clone();
                merged.extend(override_rules.clone());
                Self::Rules { rules: merged }
            }

            // Base is Recommended, override has rules - convert to rules and merge
            (
                Self::Recommended(_),
                Self::Rules {
                    rules: override_rules,
                },
            ) => {
                let mut merged = HashMap::new();
                merged.insert(
                    "recommended".to_string(),
                    LintRuleConfig::Severity(LintSeverity::Error),
                );
                merged.extend(override_rules.clone());
                Self::Rules { rules: merged }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_recommended_string() {
        let yaml = r"recommended";
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(config, LintConfig::Recommended(_)));
        assert!(config.is_enabled("unique_names"));
        assert!(config.is_enabled("deprecated_field"));
    }

    #[test]
    fn test_parse_simple_rules() {
        let yaml = "\nunique_names: error\ndeprecated_field: off\n";
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("unique_names"),
            Some(LintSeverity::Error)
        );
        assert_eq!(
            config.get_severity("deprecated_field"),
            Some(LintSeverity::Off)
        );
        assert!(!config.is_enabled("deprecated_field"));
    }

    #[test]
    fn test_parse_recommended_with_overrides() {
        let yaml = "\nrecommended: error\ndeprecated_field: off\n";
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        // Should have recommended rules enabled
        assert!(config.is_enabled("unique_names"));
        // But deprecated_field is overridden to off
        assert!(!config.is_enabled("deprecated_field"));
    }

    #[test]
    fn test_default_no_rules_enabled() {
        let config = LintConfig::default();
        assert!(!config.is_enabled("unique_names"));
        assert!(!config.is_enabled("deprecated_field"));
    }

    #[test]
    fn test_recommended_constructor() {
        let config = LintConfig::recommended();
        assert_eq!(
            config.get_severity("unique_names"),
            Some(LintSeverity::Error)
        );
        assert_eq!(
            config.get_severity("deprecated_field"),
            Some(LintSeverity::Warn)
        );
    }

    #[test]
    fn test_merge_override_rules() {
        let base = LintConfig::recommended();
        let override_yaml = "\nunused_fields: error\n";
        let override_config: LintConfig = serde_yaml::from_str(override_yaml).unwrap();

        let merged = base.merge(&override_config);

        // Should have recommended rules
        assert!(merged.is_enabled("unique_names"));
        assert!(merged.is_enabled("deprecated_field"));
        // Plus override
        assert!(merged.is_enabled("unused_fields"));
    }

    #[test]
    fn test_merge_override_severity() {
        let base_yaml = "\nunique_names: error\ndeprecated_field: warn\n";
        let base: LintConfig = serde_yaml::from_str(base_yaml).unwrap();

        let override_yaml = "\ndeprecated_field: off\n";
        let override_config: LintConfig = serde_yaml::from_str(override_yaml).unwrap();

        let merged = base.merge(&override_config);

        // unique_names unchanged
        assert_eq!(
            merged.get_severity("unique_names"),
            Some(LintSeverity::Error)
        );
        // deprecated_field overridden
        assert_eq!(
            merged.get_severity("deprecated_field"),
            Some(LintSeverity::Off)
        );
    }
}
