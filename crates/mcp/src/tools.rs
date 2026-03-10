//! MCP tool definitions
//!
//! This module defines the tools exposed to AI agents via MCP.

use crate::service::McpService;
use crate::types::{
    DocumentSymbolsParams, FileDiagnosticsParams, FilePositionParams, FindReferencesParams,
    ValidateDocumentParams, WorkspaceSymbolsParams,
};
use rmcp::handler::server::tool::{ToolCallContext, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo,
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

    #[tool(
        name = "goto_definition",
        description = "Go to the definition of a GraphQL symbol (type, field, fragment, etc.) at a given position in a loaded project file. Returns JSON with {locations[{file, range}], count}."
    )]
    pub async fn goto_definition(
        &self,
        params: Parameters<FilePositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.goto_definition(
            &params.0.file_path,
            params.0.line,
            params.0.character,
            params.0.project.as_deref(),
        );
        match result {
            Some(locations) => {
                let json = serde_json::to_string(&locations).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"locations":[],"count":0}"#,
            )])),
        }
    }

    #[tool(
        name = "find_references",
        description = "Find all references to a GraphQL symbol at a given position across the project. Returns JSON with {locations[{file, range}], count}."
    )]
    pub async fn find_references(
        &self,
        params: Parameters<FindReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.find_references(
            &params.0.file_path,
            params.0.line,
            params.0.character,
            params.0.include_declaration,
            params.0.project.as_deref(),
        );
        match result {
            Some(locations) => {
                let json = serde_json::to_string(&locations).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"locations":[],"count":0}"#,
            )])),
        }
    }

    #[tool(
        name = "hover",
        description = "Get hover information (type details, documentation) for a GraphQL symbol at a given position. Returns JSON with {contents, range?} where contents is markdown."
    )]
    pub async fn hover(
        &self,
        params: Parameters<FilePositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.hover(
            &params.0.file_path,
            params.0.line,
            params.0.character,
            params.0.project.as_deref(),
        );
        match result {
            Some(hover) => {
                let json = serde_json::to_string(&hover).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"contents":"No hover information available"}"#,
            )])),
        }
    }

    #[tool(
        name = "document_symbols",
        description = "Get all symbols (types, operations, fragments, fields) in a GraphQL file as a hierarchical outline. Returns JSON with {symbols[{name, kind, detail?, range, selection_range, children[]}], count}."
    )]
    pub async fn document_symbols(
        &self,
        params: Parameters<DocumentSymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.document_symbols(&params.0.file_path, params.0.project.as_deref());
        match result {
            Some(symbols) => {
                let json = serde_json::to_string(&symbols).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"symbols":[],"count":0}"#,
            )])),
        }
    }

    #[tool(
        name = "workspace_symbols",
        description = "Search for GraphQL symbols across all files in the workspace. Returns JSON with {symbols[{name, kind, location, container_name?}], count}."
    )]
    pub async fn workspace_symbols(
        &self,
        params: Parameters<WorkspaceSymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.workspace_symbols(&params.0.query, params.0.project.as_deref());
        match result {
            Some(symbols) => {
                let json = serde_json::to_string(&symbols).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"symbols":[],"count":0}"#,
            )])),
        }
    }

    #[tool(
        name = "get_completions",
        description = "Get completion suggestions at a position in a GraphQL file. Returns JSON with {items[{label, kind, detail?, documentation?, deprecated?}], count}."
    )]
    pub async fn get_completions(
        &self,
        params: Parameters<FilePositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.completions(
            &params.0.file_path,
            params.0.line,
            params.0.character,
            params.0.project.as_deref(),
        );
        match result {
            Some(completions) => {
                let json = serde_json::to_string(&completions).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"items":[],"count":0}"#,
            )])),
        }
    }

    #[tool(
        name = "get_file_diagnostics",
        description = "Get diagnostics (errors, warnings) for a specific file in the loaded project. Returns JSON with {file, diagnostics[{severity, message, range?, rule?, fix?}]}."
    )]
    pub async fn get_file_diagnostics(
        &self,
        params: Parameters<FileDiagnosticsParams>,
    ) -> Result<CallToolResult, McpError> {
        let service = self.service.lock().await;
        let result = service.file_diagnostics(&params.0.file_path, params.0.project.as_deref());
        match result {
            Some(diagnostics) => {
                let json = serde_json::to_string(&diagnostics).unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                r#"{"file":"","diagnostics":[]}"#,
            )])),
        }
    }
}

impl ServerHandler for GraphQLToolRouter {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "graphql-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "GraphQL MCP server providing schema-aware validation, linting, and code intelligence. \
                 Use validate_document to check if GraphQL operations are valid. \
                 Use lint_document to get best practice suggestions. \
                 Use goto_definition, find_references, hover, document_symbols, workspace_symbols, \
                 and get_completions for code navigation on loaded project files. \
                 Use get_file_diagnostics for per-file diagnostics.",
            )
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
            meta: None,
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_call_context = ToolCallContext::new(self, request, context);
        self.tool_router.call(tool_call_context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_params_serialization() {
        let params = ValidateParams {
            document: "query { user { id } }".to_string(),
            file_path: Some("query.graphql".to_string()),
            project: None,
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("query { user { id } }"));
        assert!(json.contains("query.graphql"));
    }

    #[test]
    fn test_validate_params_deserialization() {
        let json = r#"{"document": "query { user }", "file_path": "test.graphql"}"#;
        let params: ValidateParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.document, "query { user }");
        assert_eq!(params.file_path, Some("test.graphql".to_string()));
        assert!(params.project.is_none());
    }

    #[test]
    fn test_validate_params_minimal() {
        let json = r#"{"document": "query { user }"}"#;
        let params: ValidateParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.document, "query { user }");
        assert!(params.file_path.is_none());
        assert!(params.project.is_none());
    }

    #[test]
    fn test_lint_params_serialization() {
        let params = LintParams {
            document: "query GetUser { user { id } }".to_string(),
            file_path: None,
            project: Some("default".to_string()),
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("GetUser"));
        assert!(json.contains("default"));
    }

    #[test]
    fn test_lint_params_deserialization() {
        let json = r#"{"document": "mutation { createUser }", "project": "api"}"#;
        let params: LintParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.document, "mutation { createUser }");
        assert_eq!(params.project, Some("api".to_string()));
    }

    #[test]
    fn test_load_project_params_serialization() {
        let params = LoadProjectParams {
            project: "my-project".to_string(),
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("my-project"));
    }

    #[test]
    fn test_load_project_params_deserialization() {
        let json = r#"{"project": "backend-api"}"#;
        let params: LoadProjectParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.project, "backend-api");
    }
}
