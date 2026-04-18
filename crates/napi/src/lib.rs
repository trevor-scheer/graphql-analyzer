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
