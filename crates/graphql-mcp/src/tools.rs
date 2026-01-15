//! MCP tool definitions
//!
//! This module defines the tools exposed to AI agents via MCP.

use crate::service::McpService;
use crate::types::{LintResult, ValidateDocumentParams, ValidateDocumentResult};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Implementation, ServerInfo};
use rmcp::schemars::JsonSchema;
use rmcp::tool;
use rmcp::{ErrorData as McpError, ServerHandler};
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
}

#[rmcp::tool_router]
impl GraphQLToolRouter {
    /// Validate a GraphQL document against the schema
    ///
    /// This tool validates GraphQL syntax and checks that the document is valid
    /// according to the GraphQL specification. It returns any errors found,
    /// including syntax errors, unknown types/fields, and validation errors.
    #[tool(
        name = "validate_document",
        description = "Validate a GraphQL document (query, mutation, subscription, or fragment) against the loaded schema. Returns syntax errors, unknown field errors, type errors, and other validation issues."
    )]
    pub async fn validate_document(
        &self,
        params: Parameters<ValidateParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut service = self.service.lock().await;

        let result = service.validate_document(ValidateDocumentParams {
            document: params.0.document,
            file_path: params.0.file_path,
        });

        // Format as human-readable text for the AI
        let text = format_validation_result(&result);

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Lint a GraphQL document for best practices
    ///
    /// This tool runs lint rules on the document and returns any violations.
    /// Lint rules check for best practices like naming conventions, deprecated
    /// field usage, and code quality issues.
    #[tool(
        name = "lint_document",
        description = "Run lint rules on a GraphQL document to check for best practices and code quality issues. Returns warnings about naming conventions, deprecated fields, unused variables, and other potential problems."
    )]
    pub async fn lint_document(
        &self,
        params: Parameters<LintParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut service = self.service.lock().await;

        let result = service.lint_document(&params.0.document, params.0.file_path.as_deref());

        let text = format_lint_result(&result);

        Ok(CallToolResult::success(vec![Content::text(text)]))
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
            instructions: Some(
                "GraphQL MCP server providing schema-aware validation, linting, and code intelligence. \
                 Use validate_document to check if GraphQL operations are valid. \
                 Use lint_document to get best practice suggestions."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

/// Format validation result as human-readable text
fn format_validation_result(result: &ValidateDocumentResult) -> String {
    let mut output = String::new();

    if result.valid {
        output.push_str("✓ Document is valid\n");
    } else {
        output.push_str(&format!("✗ Document has {} error(s)", result.error_count));
        if result.warning_count > 0 {
            output.push_str(&format!(" and {} warning(s)", result.warning_count));
        }
        output.push('\n');
    }

    if !result.diagnostics.is_empty() {
        output.push_str("\nDiagnostics:\n");
        for diag in &result.diagnostics {
            let severity_icon = match diag.severity {
                crate::types::DiagnosticSeverity::Error => "❌",
                crate::types::DiagnosticSeverity::Warning => "⚠️",
                crate::types::DiagnosticSeverity::Info => "ℹ️",
                crate::types::DiagnosticSeverity::Hint => "💡",
            };

            if let Some(ref range) = diag.range {
                output.push_str(&format!(
                    "  {} Line {}:{} - {}\n",
                    severity_icon,
                    range.start.line + 1,
                    range.start.character + 1,
                    diag.message
                ));
            } else {
                output.push_str(&format!("  {} {}\n", severity_icon, diag.message));
            }
        }
    }

    output
}

/// Format lint result as human-readable text
fn format_lint_result(result: &LintResult) -> String {
    let mut output = String::new();

    if result.issue_count == 0 {
        output.push_str("✓ No lint issues found\n");
    } else {
        output.push_str(&format!("Found {} lint issue(s)", result.issue_count));
        if result.fixable_count > 0 {
            output.push_str(&format!(" ({} auto-fixable)", result.fixable_count));
        }
        output.push('\n');
    }

    if !result.diagnostics.is_empty() {
        output.push_str("\nIssues:\n");
        for diag in &result.diagnostics {
            let severity_icon = match diag.severity {
                crate::types::DiagnosticSeverity::Error => "❌",
                crate::types::DiagnosticSeverity::Warning => "⚠️",
                _ => "ℹ️",
            };

            if let Some(ref range) = diag.range {
                output.push_str(&format!(
                    "  {} Line {}:{} - {} [{}]\n",
                    severity_icon,
                    range.start.line + 1,
                    range.start.character + 1,
                    diag.message,
                    diag.rule.as_deref().unwrap_or("unknown")
                ));
            } else {
                output.push_str(&format!(
                    "  {} {} [{}]\n",
                    severity_icon,
                    diag.message,
                    diag.rule.as_deref().unwrap_or("unknown")
                ));
            }

            // Show fix suggestion if available
            if let Some(ref fix) = diag.fix {
                output.push_str(&format!("    💡 Fix: {}\n", fix.description));
            }
        }
    }

    output
}
