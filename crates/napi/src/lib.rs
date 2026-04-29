mod host;
mod types;

use napi_derive::napi;

pub use types::*;

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn get_rules() -> Vec<JsRuleMeta> {
    graphql_linter::all_rule_info()
        .into_iter()
        .map(JsRuleMeta::from)
        .collect()
}

#[napi]
pub fn init(config_path: String) -> napi::Result<()> {
    let path = std::path::Path::new(&config_path);
    let mut host = host::get_host().lock();
    host.init_from_config(path)
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}

/// Lint a file, optionally layering per-rule overrides on top of the
/// persistent config for the duration of the call.
///
/// `overrides_json` is a JSON-encoded `{ ruleName: ruleConfig, ... }` map.
/// Each entry deserializes as a `LintRuleConfig` (a bare severity string, an
/// array `[severity, options]`, or an object `{ severity, options }`) and
/// fully replaces the persistent config for that rule. JSON-string transport
/// keeps the napi signature simple — callers (e.g. ESLint's rule visitor)
/// just `JSON.stringify` their overrides.
#[napi]
pub fn lint_file(
    path: String,
    source: String,
    overrides_json: Option<String>,
) -> napi::Result<Vec<JsDiagnostic>> {
    let overrides =
        match overrides_json {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str::<
                    std::collections::HashMap<String, graphql_linter::LintRuleConfig>,
                >(&s)
                .map_err(|e| napi::Error::from_reason(format!("invalid overrides: {e}")))?,
            ),
            _ => None,
        };
    let mut host = host::get_host().lock();
    let diagnostics = host.lint_file(&path, &source, overrides);
    Ok(diagnostics.into_iter().map(JsDiagnostic::from).collect())
}

#[napi]
pub fn extract_graphql(source: String, language: String) -> napi::Result<Vec<JsExtractedBlock>> {
    let lang = match language.as_str() {
        "ts" | "tsx" | "typescript" => graphql_extract::Language::TypeScript,
        "js" | "jsx" | "mjs" | "cjs" | "javascript" => graphql_extract::Language::JavaScript,
        "vue" => graphql_extract::Language::Vue,
        "svelte" => graphql_extract::Language::Svelte,
        "astro" => graphql_extract::Language::Astro,
        "graphql" | "gql" => graphql_extract::Language::GraphQL,
        other => {
            return Err(napi::Error::from_reason(format!(
                "Unsupported language: {other}"
            )));
        }
    };

    // Honor the project's `extractConfig` (custom template tags, etc.) when
    // `init()` has been called; fall back to defaults otherwise so this
    // function still works standalone.
    let config = host::get_host().lock().extract_config();
    let blocks = graphql_extract::extract_from_source(&source, lang, &config, "<input>")
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    Ok(blocks.into_iter().map(JsExtractedBlock::from).collect())
}
