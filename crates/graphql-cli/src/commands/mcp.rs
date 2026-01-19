//! MCP server command
//!
//! Starts an MCP (Model Context Protocol) server for AI agent integration.

use anyhow::Result;
use graphql_mcp::McpPreloadConfig;
use std::path::PathBuf;

/// Run the MCP server
///
/// This starts an MCP server that exposes GraphQL tooling to AI agents.
/// The server communicates via stdio by default.
pub async fn run(
    workspace: Option<PathBuf>,
    no_preload: bool,
    projects: Option<Vec<String>>,
) -> Result<()> {
    let workspace = workspace.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let preload_config = if no_preload {
        McpPreloadConfig::None
    } else if let Some(projects) = projects {
        McpPreloadConfig::Selected(projects)
    } else {
        McpPreloadConfig::All
    };

    graphql_mcp::GraphQLMcpServer::run_standalone(&workspace, preload_config).await
}
