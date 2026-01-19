//! MCP tool definitions
//!
//! This module defines the tools exposed to AI agents via MCP.

use crate::service::McpService;
use crate::types::ValidateDocumentParams;
use rmcp::handler::server::tool::{ToolCallContext, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParam, ServerCapabilities, ServerInfo,
};
use rmcp::schemars::JsonSchema;
use rmcp::service::RequestContext;
use rmcp::tool;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// GraphQL MCP tool router
///
/// This struct implements `ServerHandler` and exposes GraphQL analysis tools.
#[derive(Clone)]
pub struct GraphQLToolRouter {
    service: Arc<Mutex<McpService>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl GraphQLToolRouter {
    /// Create a new tool router with the given service
    pub fn new(service: Arc<Mutex<McpService>>) -> Self {
        Self {
            service,
            tool_router: Self::tool_router(),
        }
    }
}

/// Parameters for validate_document tool (MCP-friendly wrapper)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidateParams {
    /// The GraphQL document content to validate
    #[schemars(
        description = "The GraphQL document (query, mutation, subscription, or fragment) to validate against the schema"
    )]
    pub document: String,

    /// Optional file path for better error messages
    #[schemars(description = "Optional file path for the document (used in error messages)")]
    #[serde(default)]
    pub file_path: Option<String>,

    /// Optional project name to validate against
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project. Use list_projects to see available projects."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

/// Parameters for lint_document tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LintParams {
    /// The GraphQL document content to lint
    #[schemars(description = "The GraphQL document to check against lint rules")]
    pub document: String,

    /// Optional file path for better error messages
    #[schemars(description = "Optional file path for the document")]
    #[serde(default)]
    pub file_path: Option<String>,

    /// Optional project name to lint against
    #[schemars(
        description = "Optional project name. If not provided, uses the first/only loaded project. Use list_projects to see available projects."
    )]
    #[serde(default)]
    pub project: Option<String>,
}

/// Parameters for load_project tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadProjectParams {
    /// The project name to load
    #[schemars(
        description = "The project name to load. Use list_projects to see available projects."
    )]
    pub project: String,
}

#[rmcp::tool_router]
impl GraphQLToolRouter {
    #[tool(
        name = "validate_document",
        description = "Validate a GraphQL document against the loaded schema. Returns JSON with {valid, error_count, warning_count, diagnostics[]}."
    )]
    pub async fn validate_document(
        &self,
        params: Parameters<ValidateParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut service = self.service.lock().await;
        let result = service.validate_document(ValidateDocumentParams {
            document: params.0.document,
            file_path: params.0.file_path,
            project: params.0.project,
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "lint_document",
        description = "Run lint rules on a GraphQL document. Returns JSON with {issue_count, fixable_count, diagnostics[]}."
    )]
    pub async fn lint_document(
        &self,
        params: Parameters<LintParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut service = self.service.lock().await;
        let result = service.lint_document(
            &params.0.document,
            params.0.file_path.as_deref(),
            params.0.project.as_deref(),
        );
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "list_projects",
        description = "List GraphQL projects in workspace. Returns JSON array of {name, is_loaded}."
    )]
    pub async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let projects = service.list_projects();
        let result: Vec<_> = projects
            .into_iter()
            .map(|(name, is_loaded)| serde_json::json!({"name": name, "is_loaded": is_loaded}))
            .collect();
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "load_project",
        description = "Load a specific GraphQL project by name. Use list_projects to see available projects. Returns JSON with {success, project, message}."
    )]
    pub async fn load_project(
        &self,
        params: Parameters<LoadProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut service = self.service.lock().await;
        let result = service.load_project(&params.0.project);
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "get_project_diagnostics",
        description = "Get all diagnostics for the loaded project. Returns JSON with {project, total_count, file_count, files[{file, diagnostics[]}]}. Only files with issues included."
    )]
    pub async fn get_project_diagnostics(&self) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.project_diagnostics();
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

impl ServerHandler for GraphQLToolRouter {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "graphql-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "GraphQL MCP server providing schema-aware validation, linting, and code intelligence. \
                 Use validate_document to check if GraphQL operations are valid. \
                 Use lint_document to get best practice suggestions."
                    .to_string(),
            ),
            ..Default::default()
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_call_context = ToolCallContext::new(self, request, context);
        self.tool_router.call(tool_call_context).await
    }
}
