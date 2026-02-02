//! Standalone GraphQL Language Server binary
//!
//! This is a thin wrapper that starts the GraphQL LSP server.
//! For CLI usage with additional commands, use `graphql lsp` instead.

#[tokio::main]
async fn main() {
    graphql_lsp::run_server().await;
}
