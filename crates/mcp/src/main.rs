//! Standalone GraphQL MCP Server binary
//!
//! This is a thin wrapper that starts the GraphQL MCP server.
//! For CLI usage with additional options, use `graphql mcp` instead.

use graphql_mcp::McpPreloadConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let workspace = std::env::current_dir().unwrap_or_default();
    graphql_mcp::GraphQLMcpServer::run_standalone(&workspace, McpPreloadConfig::All).await
}
