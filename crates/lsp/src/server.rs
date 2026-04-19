use lsp_types::Diagnostic;

/// Parameters for the `graphql/virtualFileContent` custom request.
#[derive(Debug, serde::Deserialize)]
pub struct VirtualFileContentParams {
    pub uri: String,
}

/// Custom notification sent from server to client to indicate loading status.
pub enum StatusNotification {}

impl lsp_types::notification::Notification for StatusNotification {
    type Params = StatusParams;
    const METHOD: &'static str = "graphql-analyzer/status";
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StatusParams {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response for the `graphql/ping` health check request.
#[derive(Debug, serde::Serialize)]
pub struct PingResponse {
    pub timestamp: u64,
}

/// Convert config validation errors to LSP diagnostics.
pub fn validation_errors_to_diagnostics(
    errors: &[graphql_config::ConfigValidationError],
    config_content: &str,
) -> Vec<Diagnostic> {
    errors
        .iter()
        .map(|error| {
            let range = error
                .location(config_content)
                .map_or(lsp_types::Range::default(), |loc| lsp_types::Range {
                    start: lsp_types::Position {
                        line: loc.line,
                        character: loc.start_column,
                    },
                    end: lsp_types::Position {
                        line: loc.line,
                        character: loc.end_column,
                    },
                });

            let severity = match error.severity() {
                graphql_config::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
                graphql_config::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            };

            Diagnostic {
                range,
                severity: Some(severity),
                code: Some(lsp_types::NumberOrString::String(error.code().to_string())),
                source: Some("graphql-config".to_string()),
                message: error.message(),
                ..Default::default()
            }
        })
        .collect()
}
