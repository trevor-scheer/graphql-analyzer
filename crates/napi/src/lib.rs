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

#[napi]
pub fn lint_file(path: String, source: String) -> napi::Result<Vec<JsDiagnostic>> {
    let mut host = host::get_host().lock();
    let diagnostics = host.lint_file(&path, &source);
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
