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
