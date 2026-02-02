//! Standalone GraphQL MCP Server binary
//!
//! This is a thin wrapper that starts the GraphQL MCP server.
//! For CLI usage with additional options, use `graphql mcp` instead.

use clap::Parser;
use graphql_mcp::McpPreloadConfig;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "graphql-mcp")]
#[command(about = "GraphQL MCP server for AI agent integration")]
#[command(version)]
struct Args {
    /// Workspace directory (defaults to current directory)
    #[arg(short, long)]
    workspace: Option<PathBuf>,

    /// Don't preload any projects (use `load_project` tool to load on demand)
    #[arg(long)]
    no_preload: bool,

    /// Specific projects to preload (comma-separated)
    #[arg(long, value_delimiter = ',')]
    preload: Option<Vec<String>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let workspace = args
        .workspace
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let preload_config = if args.no_preload {
        McpPreloadConfig::None
    } else if let Some(projects) = args.preload {
        McpPreloadConfig::Selected(projects)
    } else {
        McpPreloadConfig::All
    };

    graphql_mcp::GraphQLMcpServer::run_standalone(&workspace, preload_config).await
}
