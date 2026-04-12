use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// Severity level for a lint rule
///
/// Custom deserializer handles YAML 1.1 boolean coercion where bare `off`
/// is parsed as boolean `false` by spec-compliant YAML parsers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum LintSeverity {
    Off,
    Warn,
    Error,
}

impl<'de> Deserialize<'de> for LintSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SeverityVisitor;

        impl Visitor<'_> for SeverityVisitor {
            type Value = LintSeverity;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a severity: 'off', 'warn', or 'error'")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                if v {
                    Err(E::custom("boolean `true` is not a valid severity"))
                } else {
                    Ok(LintSeverity::Off)
                }
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v {
                    "off" => Ok(LintSeverity::Off),
                    "warn" => Ok(LintSeverity::Warn),
                    "error" => Ok(LintSeverity::Error),
                    _ => Err(E::custom(format!("unknown severity: {v}"))),
                }
            }
        }

        deserializer.deserialize_any(SeverityVisitor)
    }
}

impl std::fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Configuration for a single lint rule
///
/// Supports multiple formats:
/// ```yaml
/// # Simple severity
/// rule_name: warn
///
/// # Object style with options
/// rule_name:
///   severity: warn
///   options:
///     field_name: ["id", "uuid"]
///
/// # ESLint-style array: [severity, options]
/// rule_name: [warn, { field_name: ["id", "uuid"] }]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LintRuleConfig {
    /// Just a severity level (simple case)
    Severity(LintSeverity),

    /// Detailed config with options
    Detailed {
        severity: LintSeverity,
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<serde_json::Value>,
    },
}

impl LintRuleConfig {
    /// Get the severity for this rule configuration
    #[must_use]
    pub fn severity(&self) -> LintSeverity {
        match self {
            Self::Severity(s) | Self::Detailed { severity: s, .. } => *s,
        }
    }

    /// Get the options for this rule configuration (if any)
    #[must_use]
    pub fn options(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Severity(_) => None,
            Self::Detailed { options, .. } => options.as_ref(),
        }
    }
}

/// Custom deserializer for `LintRuleConfig` to handle ESLint-style array syntax
impl<'de> Deserialize<'de> for LintRuleConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LintRuleConfigVisitor;

        impl<'de> Visitor<'de> for LintRuleConfigVisitor {
            type Value = LintRuleConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a severity string ('off', 'warn', 'error'), \
                     an array [severity, options], \
                     or an object { severity, options }",
                )
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // YAML 1.1 treats `off`/`no`/`false` as boolean false and
                // `on`/`yes`/`true` as boolean true. Map false → Off severity.
                if value {
                    Err(E::custom(
                        "boolean `true` is not a valid severity; use 'off', 'warn', or 'error'",
                    ))
                } else {
                    Ok(LintRuleConfig::Severity(LintSeverity::Off))
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let severity = match value {
                    "off" => LintSeverity::Off,
                    "warn" => LintSeverity::Warn,
                    "error" => LintSeverity::Error,
                    _ => return Err(E::custom(format!("unknown severity: {value}"))),
                };
                Ok(LintRuleConfig::Severity(severity))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                // ESLint-style: [severity, options]
                let severity: LintSeverity = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"array with severity"))?;

                let options: Option<serde_json::Value> = seq.next_element()?;

                Ok(LintRuleConfig::Detailed { severity, options })
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                // Object style: { severity, options }
                #[derive(Deserialize)]
                struct DetailedConfig {
                    severity: LintSeverity,
                    #[serde(default)]
                    options: Option<serde_json::Value>,
                }

                let config =
                    DetailedConfig::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(LintRuleConfig::Detailed {
                    severity: config.severity,
                    options: config.options,
                })
            }
        }

        deserializer.deserialize_any(LintRuleConfigVisitor)
    }
}

/// Extends configuration - can be a single preset or multiple
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ExtendsConfig {
    /// Single preset: `extends: recommended` or `lint: recommended`
    Single(String),
    /// Multiple presets: `extends: [recommended, strict]` or `lint: [recommended, strict]`
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
    ///
    /// Rule names use `camelCase` (e.g., `noDeprecated`), matching the config file format.
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
/// # Multiple presets
/// lint: [recommended, strict]
///
/// # Fine-grained rules only (no presets)
/// lint:
///   rules:
///     uniqueNames: error
///     noDeprecated: warn
///
/// # Preset with overrides
/// lint:
///   extends: recommended
///   rules:
///     noDeprecated: off
///
/// # Multiple presets with overrides
/// lint:
///   extends: [recommended, strict]
///   rules:
///     requireIdField: off
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum LintConfig {
    /// Preset(s): `lint: recommended` or `lint: [recommended, strict]`
    Preset(ExtendsConfig),

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
            Self::Preset(presets) => {
                for preset in presets.presets() {
                    if !valid_presets.contains(&preset) {
                        return Err(format!(
                            "Invalid preset name: '{preset}'\n\nValid presets are:\n  - recommended"
                        ));
                    }
                }
                return Ok(());
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
            Self::Preset(presets) => Self::severity_from_presets(presets, rule_name),
            Self::Full(FullLintConfig { extends, rules }) => {
                // Start with preset severities (if any)
                let preset_severity = extends
                    .as_ref()
                    .and_then(|ext| Self::severity_from_presets(ext, rule_name));

                // Check for explicit rule override
                rules
                    .get(rule_name)
                    .map(LintRuleConfig::severity)
                    .or(preset_severity)
            }
        }
    }

    /// Get the options for a rule (if configured)
    ///
    /// Returns `None` if the rule is not configured or has no options.
    #[must_use]
    pub fn get_options(&self, rule_name: &str) -> Option<&serde_json::Value> {
        match self {
            Self::Preset(_) => None,
            Self::Full(FullLintConfig { rules, .. }) => {
                rules.get(rule_name).and_then(LintRuleConfig::options)
            }
        }
    }

    /// Get severity from a list of presets (later presets override earlier)
    fn severity_from_presets(presets: &ExtendsConfig, rule_name: &str) -> Option<LintSeverity> {
        let mut severity = None;
        for preset in presets.presets() {
            if preset == "recommended" {
                if let Some(s) = Self::recommended_severity(rule_name) {
                    severity = Some(s);
                }
            }
            // Add more presets here as they're added
        }
        severity
    }

    /// Check if a rule is enabled (not Off and not None)
    #[must_use]
    pub fn is_enabled(&self, rule_name: &str) -> bool {
        matches!(
            self.get_severity(rule_name),
            Some(LintSeverity::Warn | LintSeverity::Error)
        )
    }

    /// Get recommended severity for a rule.
    ///
    /// The `recommended` preset includes rules that are objectively beneficial
    /// without being opinionated about architecture choices.
    fn recommended_severity(rule_name: &str) -> Option<LintSeverity> {
        match rule_name {
            "noAnonymousOperations" => Some(LintSeverity::Error),
            "noDeprecated"
            | "redundantFields"
            | "unusedFragments"
            | "unusedFields"
            | "noDuplicateFields"
            | "noUnreachableTypes"
            | "requireDeprecationReason"
            | "noHashtagDescription"
            | "uniqueEnumValueNames" => Some(LintSeverity::Warn),
            _ => None,
        }
    }

    /// Get recommended configuration
    #[must_use]
    pub fn recommended() -> Self {
        Self::Preset(ExtendsConfig::Single("recommended".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_preset() {
        let yaml = r"recommended";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert!(matches!(
            config,
            LintConfig::Preset(ExtendsConfig::Single(ref s)) if s == "recommended"
        ));
        // uniqueNames is not in recommended (it's opinionated - only needed for PQs)
        assert!(!config.is_enabled("uniqueNames"));
        assert!(config.is_enabled("noDeprecated"));
        assert!(config.is_enabled("unusedFields"));
        assert!(config.is_enabled("noDuplicateFields"));
        assert!(config.is_enabled("noUnreachableTypes"));
        assert!(config.is_enabled("requireDeprecationReason"));
        assert!(config.is_enabled("noHashtagDescription"));
        assert!(config.is_enabled("uniqueEnumValueNames"));
    }

    #[test]
    fn test_preset_list() {
        let yaml = r"[recommended]";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert!(matches!(
            config,
            LintConfig::Preset(ExtendsConfig::Multiple(_))
        ));
        // uniqueNames is not in recommended (it's opinionated - only needed for PQs)
        assert!(!config.is_enabled("uniqueNames"));
        assert!(config.is_enabled("noDeprecated"));
    }

    #[test]
    fn test_rules_only() {
        let yaml = r"
rules:
  uniqueNames: error
  noDeprecated: warn
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("uniqueNames"),
            Some(LintSeverity::Error)
        );
        assert_eq!(
            config.get_severity("noDeprecated"),
            Some(LintSeverity::Warn)
        );
        assert_eq!(config.get_severity("requireIdField"), None);
    }

    #[test]
    fn test_extends_single() {
        let yaml = r"
extends: recommended
rules:
  noDeprecated: off
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        // uniqueNames and requireIdField are not in recommended (opinionated rules)
        assert!(!config.is_enabled("uniqueNames"));
        assert!(!config.is_enabled("noDeprecated"));
        assert!(!config.is_enabled("requireIdField"));
    }

    #[test]
    fn test_extends_multiple() {
        let yaml = r"
extends: [recommended]
rules:
  unusedFields: warn
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        // uniqueNames is not in recommended (opinionated)
        assert!(!config.is_enabled("uniqueNames"));
        assert!(config.is_enabled("unusedFields"));
    }

    #[test]
    fn test_extends_with_override() {
        let yaml = r"
extends: recommended
rules:
  uniqueNames: warn
  requireIdField: off
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(config.get_severity("uniqueNames"), Some(LintSeverity::Warn));
        assert_eq!(
            config.get_severity("requireIdField"),
            Some(LintSeverity::Off)
        );
        assert_eq!(
            config.get_severity("noDeprecated"),
            Some(LintSeverity::Warn)
        );
    }

    #[test]
    fn test_validate_valid_preset() {
        let config = LintConfig::Preset(ExtendsConfig::Single("recommended".to_string()));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_preset() {
        let config = LintConfig::Preset(ExtendsConfig::Single("not_a_preset".to_string()));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_rule() {
        let yaml = r"
rules:
  notARule: error
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("notARule"));
    }

    #[test]
    fn test_default_no_rules_enabled() {
        let config = LintConfig::default();
        assert!(!config.is_enabled("uniqueNames"));
        assert!(!config.is_enabled("noDeprecated"));
    }

    #[test]
    fn test_recommended_constructor() {
        let config = LintConfig::recommended();
        // uniqueNames is not in recommended (opinionated - only needed for PQs)
        assert_eq!(config.get_severity("uniqueNames"), None);
        assert_eq!(
            config.get_severity("noDeprecated"),
            Some(LintSeverity::Warn)
        );
    }

    #[test]
    fn test_eslint_array_style() {
        let yaml = r#"
rules:
  requireIdField: [warn, { fields: ["id", "nodeId"] }]
"#;
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("requireIdField"),
            Some(LintSeverity::Warn)
        );

        let options = config.get_options("requireIdField").unwrap();
        let fields = options.get("fields").unwrap().as_array().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].as_str().unwrap(), "id");
        assert_eq!(fields[1].as_str().unwrap(), "nodeId");
    }

    #[test]
    fn test_eslint_array_style_severity_only() {
        let yaml = r"
rules:
  requireIdField: [error]
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("requireIdField"),
            Some(LintSeverity::Error)
        );
        assert!(config.get_options("requireIdField").is_none());
    }

    #[test]
    fn test_object_style_with_options() {
        let yaml = r#"
rules:
  requireIdField:
    severity: warn
    options:
      fields: ["id", "uuid"]
"#;
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(
            config.get_severity("requireIdField"),
            Some(LintSeverity::Warn)
        );

        let options = config.get_options("requireIdField").unwrap();
        let fields = options.get("fields").unwrap().as_array().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].as_str().unwrap(), "id");
        assert_eq!(fields[1].as_str().unwrap(), "uuid");
    }

    #[test]
    fn test_get_options_returns_none_for_simple_severity() {
        let yaml = r"
rules:
  requireIdField: warn
";
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();
        assert!(config.get_options("requireIdField").is_none());
    }

    #[test]
    fn test_get_options_returns_none_for_preset() {
        let config = LintConfig::recommended();
        assert!(config.get_options("requireIdField").is_none());
    }

    #[test]
    fn test_mixed_rule_configs() {
        let yaml = r#"
rules:
  noDeprecated: warn
  requireIdField: [error, { fields: ["id"] }]
  uniqueNames:
    severity: error
"#;
        let config: LintConfig = serde_saphyr::from_str(yaml).unwrap();

        // Simple severity
        assert_eq!(
            config.get_severity("noDeprecated"),
            Some(LintSeverity::Warn)
        );
        assert!(config.get_options("noDeprecated").is_none());

        // ESLint array style
        assert_eq!(
            config.get_severity("requireIdField"),
            Some(LintSeverity::Error)
        );
        assert!(config.get_options("requireIdField").is_some());

        // Object style without options
        assert_eq!(
            config.get_severity("uniqueNames"),
            Some(LintSeverity::Error)
        );
        assert!(config.get_options("uniqueNames").is_none());
    }
}
