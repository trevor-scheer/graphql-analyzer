//! GraphQL Language Server binary entry point.
//!
//! This binary is deprecated in favor of `graphql lsp`.
//! It's kept for backwards compatibility but may be removed in a future version.

#[tokio::main]
async fn main() {
    graphql_lsp::run_server().await;
}
