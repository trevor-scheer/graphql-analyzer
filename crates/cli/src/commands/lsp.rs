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
    // The LSP server is fully sync (main loop + thread pool). We run it
    // on a blocking thread so the async CLI runtime doesn't interfere.
    tokio::task::spawn_blocking(graphql_lsp::run_server)
        .await
        .expect("LSP server thread");
    Ok(())
}
