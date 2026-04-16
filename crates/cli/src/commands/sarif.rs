//! SARIF (Static Analysis Results Interchange Format) output support.
//!
//! Produces SARIF v2.1.0 JSON for integration with GitHub code scanning
//! and other SARIF-compatible tools.

use std::collections::BTreeMap;
use std::path::Path;

/// A single diagnostic result for SARIF output.
pub struct SarifResult {
    pub rule_id: String,
    pub message: String,
    pub level: SarifLevel,
    pub file_path: String,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// SARIF severity level.
pub enum SarifLevel {
    Error,
    Warning,
    Note,
}

impl SarifLevel {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

/// Build a complete SARIF v2.1.0 JSON document from a set of diagnostic results.
///
/// `base_dir` is used to compute relative artifact URIs via `%SRCROOT%`.
pub fn format_sarif(results: &[SarifResult], base_dir: &Path) -> serde_json::Value {
    // Collect unique rules (deduplicated, sorted for stable output)
    let mut rules_map: BTreeMap<&str, &str> = BTreeMap::new();
    for r in results {
        rules_map.entry(&r.rule_id).or_insert(&r.message);
    }

    let rules: Vec<serde_json::Value> = rules_map
        .keys()
        .map(|rule_id| {
            serde_json::json!({
                "id": rule_id,
                "helpUri": format!("https://graphql-analyzer.dev/rules/{rule_id}")
            })
        })
        .collect();

    // Build rule index lookup for ruleIndex references
    let rule_index: BTreeMap<&str, usize> = rules_map
        .keys()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();

    let sarif_results: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let relative_path = Path::new(&r.file_path)
                .strip_prefix(base_dir)
                .map_or_else(|_| r.file_path.clone(), |p| p.to_string_lossy().to_string());

            let mut result = serde_json::json!({
                "ruleId": r.rule_id,
                "ruleIndex": rule_index.get(r.rule_id.as_str()).copied().unwrap_or(0),
                "level": r.level.as_str(),
                "message": { "text": r.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": relative_path,
                            "uriBaseId": "%SRCROOT%"
                        },
                        "region": {
                            "startLine": r.start_line,
                            "startColumn": r.start_column,
                            "endLine": r.end_line,
                            "endColumn": r.end_column
                        }
                    }
                }]
            });

            // Ensure forward slashes in URI on all platforms
            if let Some(loc) = result
                .get_mut("locations")
                .and_then(|l| l.get_mut(0))
                .and_then(|l| l.get_mut("physicalLocation"))
                .and_then(|l| l.get_mut("artifactLocation"))
                .and_then(|l| l.get_mut("uri"))
            {
                if let Some(s) = loc.as_str() {
                    *loc = serde_json::Value::String(s.replace('\\', "/"));
                }
            }

            result
        })
        .collect();

    serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "graphql-analyzer",
                    "informationUri": "https://graphql-analyzer.dev",
                    "rules": rules
                }
            },
            "results": sarif_results
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn empty_results_produce_valid_sarif() {
        let base = PathBuf::from("/project");
        let output = format_sarif(&[], &base);

        assert_eq!(output["version"], "2.1.0");
        assert!(output["runs"][0]["results"].as_array().unwrap().is_empty());
        assert!(output["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn single_result_produces_correct_structure() {
        let base = PathBuf::from("/project");
        let results = vec![SarifResult {
            rule_id: "noAnonymousOperations".to_string(),
            message: "All operations must be named".to_string(),
            level: SarifLevel::Warning,
            file_path: "/project/src/query.graphql".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 3,
            end_column: 2,
        }];

        let output = format_sarif(&results, &base);

        let result = &output["runs"][0]["results"][0];
        assert_eq!(result["ruleId"], "noAnonymousOperations");
        assert_eq!(result["level"], "warning");
        assert_eq!(result["message"]["text"], "All operations must be named");

        let region = &result["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], 1);
        assert_eq!(region["startColumn"], 1);
        assert_eq!(region["endLine"], 3);
        assert_eq!(region["endColumn"], 2);

        let artifact = &result["locations"][0]["physicalLocation"]["artifactLocation"];
        assert_eq!(artifact["uri"], "src/query.graphql");
        assert_eq!(artifact["uriBaseId"], "%SRCROOT%");
    }

    #[test]
    fn rules_are_deduplicated() {
        let base = PathBuf::from("/project");
        let results = vec![
            SarifResult {
                rule_id: "noAnonymousOperations".to_string(),
                message: "msg1".to_string(),
                level: SarifLevel::Warning,
                file_path: "/project/a.graphql".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
            SarifResult {
                rule_id: "noAnonymousOperations".to_string(),
                message: "msg2".to_string(),
                level: SarifLevel::Warning,
                file_path: "/project/b.graphql".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
        ];

        let output = format_sarif(&results, &base);
        let rules = output["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn severity_levels_map_correctly() {
        assert_eq!(SarifLevel::Error.as_str(), "error");
        assert_eq!(SarifLevel::Warning.as_str(), "warning");
        assert_eq!(SarifLevel::Note.as_str(), "note");
    }

    #[test]
    fn path_outside_base_dir_uses_absolute() {
        let base = PathBuf::from("/project");
        let results = vec![SarifResult {
            rule_id: "test".to_string(),
            message: "msg".to_string(),
            level: SarifLevel::Error,
            file_path: "/other/file.graphql".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        }];

        let output = format_sarif(&results, &base);
        let uri = &output["runs"][0]["results"][0]["locations"][0]["physicalLocation"]
            ["artifactLocation"]["uri"];
        assert_eq!(uri, "/other/file.graphql");
    }
}
