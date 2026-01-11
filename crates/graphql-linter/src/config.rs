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

/// Extends configuration - can be a single preset or multiple
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtendsConfig {
    /// Single preset: `extends: recommended`
    Single(String),
    /// Multiple presets: `extends: [recommended, strict]`
    Multiple(Vec<String>),
}

impl ExtendsConfig {
    /// Get all presets as a vector (normalizes single to vec)
    #[must_use]
    pub fn presets(&self) -> Vec<&str> {
        match self {
            Self::Single(s) => vec![s.as_str()],
            Self::Multiple(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// Full lint configuration struct with extends and rules
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullLintConfig {
    /// Presets to extend (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<ExtendsConfig>,

    /// Rule configurations (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub rules: HashMap<String, LintRuleConfig>,
}

/// Overall lint configuration
///
/// Supports multiple formats:
///
/// ```yaml
/// # Happy path - just use recommended preset
/// lint: recommended
///
/// # Fine-grained rules only (no presets)
/// lint:
///   rules:
///     unique_names: error
///     no_deprecated: warn
///
/// # Preset with overrides
/// lint:
///   extends: recommended
///   rules:
///     no_deprecated: off
///
/// # Multiple presets (later overrides earlier)
/// lint:
///   extends: [recommended, strict]
///   rules:
///     require_id_field: off
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LintConfig {
    /// Simple preset name: `lint: recommended`
    Preset(String),

    /// Full configuration with optional extends and rules
    Full(FullLintConfig),
}

impl Default for LintConfig {
    fn default() -> Self {
        Self::Full(FullLintConfig {
            extends: None,
            rules: HashMap::new(),
        })
    }
}

impl LintConfig {
    /// Validate the lint configuration against available rules
    ///
    /// Returns an error if any configured rule names are invalid.
    /// The error message includes a list of valid rule names.
    pub fn validate(&self) -> Result<(), String> {
        let valid_rules = crate::registry::all_rule_names();
        let valid_set: std::collections::HashSet<&str> = valid_rules.iter().copied().collect();

        let valid_presets = ["recommended"];

        let rules = match self {
            Self::Preset(name) => {
                if valid_presets.contains(&name.as_str()) {
                    return Ok(());
                }
                return Err(format!(
                    "Invalid preset name: '{name}'\n\nValid presets are:\n  - recommended"
                ));
            }
            Self::Full(FullLintConfig { extends, rules }) => {
                if let Some(ext) = extends {
                    for preset in ext.presets() {
                        if !valid_presets.contains(&preset) {
                            return Err(format!(
                                "Invalid preset name: '{preset}'\n\nValid presets are:\n  - recommended"
                            ));
                        }
                    }
                }
                rules
            }
        };

        let invalid_rules: Vec<&str> = rules
            .keys()
            .map(String::as_str)
            .filter(|rule| !valid_set.contains(*rule))
            .collect();

        if invalid_rules.is_empty() {
            Ok(())
        } else {
            use std::fmt::Write;
            let mut error = format!(
                "Invalid lint rule name(s): {}\n\nValid rule names are:\n",
                invalid_rules.join(", ")
            );
            for rule in &valid_rules {
                let _ = writeln!(error, "  - {rule}");
            }
            Err(error)
        }
    }

    /// Get the severity for a rule, considering presets and overrides
    #[must_use]
    pub fn get_severity(&self, rule_name: &str) -> Option<LintSeverity> {
        match self {
            Self::Preset(name) => {
                if name == "recommended" {
                    Self::recommended_severity(rule_name)
                } else {
                    None
                }
            }
            Self::Full(FullLintConfig { extends, rules }) => {
                // Start with preset severities (if any)
                let preset_severity = extends.as_ref().and_then(|ext| {
                    let mut severity = None;
                    for preset in ext.presets() {
                        if preset == "recommended" {
                            if let Some(s) = Self::recommended_severity(rule_name) {
                                severity = Some(s);
                            }
                        }
                    }
                    severity
                });

                // Check for explicit rule override
                rules
                    .get(rule_name)
                    .map(|config| match config {
                        LintRuleConfig::Severity(severity)
                        | LintRuleConfig::Detailed { severity, .. } => *severity,
                    })
                    .or(preset_severity)
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
            "no_deprecated" | "redundant_fields" | "require_id_field" => Some(LintSeverity::Warn),
            _ => None,
        }
    }

    /// Get recommended configuration
    #[must_use]
    pub fn recommended() -> Self {
        Self::Preset("recommended".to_string())
    }

    /// Merge another config into this one (tool-specific overrides)
    #[must_use]
    pub fn merge(&self, override_config: &Self) -> Self {
        match (self, override_config) {
            // If override is a preset, use it directly
            (_, Self::Preset(name)) => Self::Preset(name.clone()),

            // If override is empty Full config, keep base
            (
                base,
                Self::Full(FullLintConfig {
                    extends: None,
                    rules,
                }),
            ) if rules.is_empty() => base.clone(),

            // Merge Full configs
            (
                Self::Full(FullLintConfig {
                    extends: base_ext,
                    rules: base_rules,
                }),
                Self::Full(FullLintConfig {
                    extends: override_ext,
                    rules: override_rules,
                }),
            ) => {
                let mut merged_rules = base_rules.clone();
                merged_rules.extend(override_rules.clone());
                Self::Full(FullLintConfig {
                    extends: override_ext.clone().or_else(|| base_ext.clone()),
                    rules: merged_rules,
                })
            }

            // Preset + Full override: convert preset to extends and merge
            (
                Self::Preset(name),
                Self::Full(FullLintConfig {
                    extends: override_ext,
                    rules: override_rules,
                }),
            ) => Self::Full(FullLintConfig {
                extends: override_ext
                    .clone()
                    .or_else(|| Some(ExtendsConfig::Single(name.clone()))),
                rules: override_rules.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_preset() {
        let yaml = r#"recommended"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(config, LintConfig::Preset(ref s) if s == "recommended"));
        assert!(config.is_enabled("unique_names"));
        assert!(config.is_enabled("no_deprecated"));
        assert!(!config.is_enabled("unused_fields"));
    }

    #[test]
    fn test_rules_only() {
        let yaml = r#"
rules:
  unique_names: error
  no_deprecated: warn
"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("unique_names"),
            Some(LintSeverity::Error)
        );
        assert_eq!(
            config.get_severity("no_deprecated"),
            Some(LintSeverity::Warn)
        );
        assert_eq!(config.get_severity("require_id_field"), None);
    }

    #[test]
    fn test_extends_single() {
        let yaml = r#"
extends: recommended
rules:
  no_deprecated: off
"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.is_enabled("unique_names"));
        assert!(!config.is_enabled("no_deprecated"));
        assert!(config.is_enabled("require_id_field"));
    }

    #[test]
    fn test_extends_multiple() {
        let yaml = r#"
extends: [recommended]
rules:
  unused_fields: warn
"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.is_enabled("unique_names"));
        assert!(config.is_enabled("unused_fields"));
    }

    #[test]
    fn test_extends_with_override() {
        let yaml = r#"
extends: recommended
rules:
  unique_names: warn
  require_id_field: off
"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("unique_names"),
            Some(LintSeverity::Warn)
        );
        assert_eq!(
            config.get_severity("require_id_field"),
            Some(LintSeverity::Off)
        );
        assert_eq!(
            config.get_severity("no_deprecated"),
            Some(LintSeverity::Warn)
        );
    }

    #[test]
    fn test_merge_preset_with_rules() {
        let base = LintConfig::recommended();
        let override_yaml = r#"
rules:
  unused_fields: error
"#;
        let override_config: LintConfig = serde_yaml::from_str(override_yaml).unwrap();
        let merged = base.merge(&override_config);

        assert!(merged.is_enabled("unique_names"));
        assert!(merged.is_enabled("unused_fields"));
    }

    #[test]
    fn test_merge_extends_override_severity() {
        let base_yaml = r#"
extends: recommended
"#;
        let base: LintConfig = serde_yaml::from_str(base_yaml).unwrap();

        let override_yaml = r#"
rules:
  no_deprecated: off
"#;
        let override_config: LintConfig = serde_yaml::from_str(override_yaml).unwrap();
        let merged = base.merge(&override_config);

        assert!(merged.is_enabled("unique_names"));
        assert!(!merged.is_enabled("no_deprecated"));
    }

    #[test]
    fn test_validate_valid_preset() {
        let config = LintConfig::Preset("recommended".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_preset() {
        let config = LintConfig::Preset("not_a_preset".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_rule() {
        let yaml = r#"
rules:
  not_a_rule: error
"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not_a_rule"));
    }

    #[test]
    fn test_default_no_rules_enabled() {
        let config = LintConfig::default();
        assert!(!config.is_enabled("unique_names"));
        assert!(!config.is_enabled("no_deprecated"));
    }

    #[test]
    fn test_recommended_constructor() {
        let config = LintConfig::recommended();
        assert_eq!(
            config.get_severity("unique_names"),
            Some(LintSeverity::Error)
        );
        assert_eq!(
            config.get_severity("no_deprecated"),
            Some(LintSeverity::Warn)
        );
    }
}
