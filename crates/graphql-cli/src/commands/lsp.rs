//! LSP server command
//!
//! Starts the GraphQL Language Server Protocol server.

use anyhow::Result;

/// Run the LSP server
///
/// This starts the GraphQL language server, communicating via stdio.
/// The server provides IDE features like diagnostics, hover, goto definition,
/// find references, and completions for GraphQL files.
pub async fn run() -> Result<()> {
    graphql_lsp::run_server().await;
    Ok(())
}
