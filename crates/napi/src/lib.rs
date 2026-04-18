mod types;

use napi_derive::napi;

pub use types::*;

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
