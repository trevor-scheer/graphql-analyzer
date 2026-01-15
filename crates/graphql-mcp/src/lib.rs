//! # graphql-mcp
//!
//! MCP (Model Context Protocol) server for GraphQL tooling.

// Allow common clippy lints for scaffolding code
#![allow(clippy::doc_markdown)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::format_push_string)]
#![allow(clippy::needless_pass_by_value)]
//!
//! This crate provides AI agents with GraphQL analysis capabilities including:
//! - Schema-aware validation
//! - Linting with auto-fix suggestions
//! - Type information and completions
//! - Schema introspection from remote endpoints
//!
//! ## Architecture
//!
//! The MCP server can run in three modes:
//!
//! 1. **Standalone binary** (`graphql-mcp`) - Independent process
//! 2. **CLI subcommand** (`graphql mcp`) - Part of the GraphQL CLI
//! 3. **Embedded in LSP** (`graphql-lsp --mcp-port 3000`) - Shares state with LSP
//!
//! All modes use the same `McpService` which wraps `graphql-ide::Analysis`.
//!
//! ## Example
//!
//! ```ignore
//! use graphql_mcp::{McpService, GraphQLMcpServer};
//!
//! // Standalone mode - owns its own AnalysisHost
//! GraphQLMcpServer::run_standalone("/path/to/project").await?;
//!
//! // Or embedded mode - share Analysis with LSP
//! let analysis = host.snapshot();
//! GraphQLMcpServer::run_with_analysis(analysis, transport).await?;
//! ```

mod service;
mod tools;
mod types;

pub use service::McpService;
pub use tools::GraphQLToolRouter;
pub use types::{
    DiagnosticInfo, DiagnosticSeverity, FileValidationResult, LintResult, LocationInfo, RangeInfo,
    ValidateDocumentParams, ValidateDocumentResult,
};

use anyhow::Result;
use rmcp::ServiceExt;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// MCP server for GraphQL tooling
///
/// This server exposes GraphQL analysis capabilities to AI agents via the
/// Model Context Protocol. It can run standalone or embedded within the LSP.
#[allow(dead_code)]
pub struct GraphQLMcpServer {
    service: Arc<Mutex<McpService>>,
}

impl GraphQLMcpServer {
    /// Create a new MCP server with a fresh AnalysisHost
    pub fn new() -> Self {
        Self {
            service: Arc::new(Mutex::new(McpService::new())),
        }
    }

    /// Create an MCP server with a shared Analysis snapshot
    ///
    /// Use this for embedded mode where the LSP and MCP share state.
    pub fn with_analysis(analysis: graphql_ide::Analysis) -> Self {
        Self {
            service: Arc::new(Mutex::new(McpService::with_analysis(analysis))),
        }
    }

    /// Run the MCP server in standalone mode with stdio transport
    ///
    /// This creates a fresh AnalysisHost and loads the project from the given workspace path.
    pub async fn run_standalone(workspace: &Path) -> Result<()> {
        tracing::info!("Starting GraphQL MCP server in standalone mode");
        tracing::info!("Workspace: {}", workspace.display());

        // Create service and load project
        let mut service = McpService::new();
        service.load_workspace(workspace)?;

        // Create the tool router with the service
        let router = GraphQLToolRouter::new(Arc::new(Mutex::new(service)));

        // Run with stdio transport
        let transport = rmcp::transport::stdio();
        let server = router.serve(transport).await?;

        tracing::info!("GraphQL MCP server running");

        // Wait for shutdown
        server.waiting().await?;

        tracing::info!("GraphQL MCP server stopped");
        Ok(())
    }

    /// Run the MCP server with a shared Analysis snapshot (embedded mode)
    ///
    /// The server will use the provided Analysis for queries. This is useful
    /// when embedding MCP in the LSP server to share cached state.
    pub async fn run_with_analysis(analysis: graphql_ide::Analysis) -> Result<()> {
        tracing::info!("Starting GraphQL MCP server in embedded mode");

        let service = McpService::with_analysis(analysis);
        let router = GraphQLToolRouter::new(Arc::new(Mutex::new(service)));

        let transport = rmcp::transport::stdio();
        let server = router.serve(transport).await?;

        tracing::info!("GraphQL MCP server running (embedded mode)");

        server.waiting().await?;

        tracing::info!("GraphQL MCP server stopped");
        Ok(())
    }
}

impl Default for GraphQLMcpServer {
    fn default() -> Self {
        Self::new()
    }
}
