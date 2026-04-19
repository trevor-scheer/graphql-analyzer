#![allow(clippy::needless_pass_by_value)]

use crate::global_state::GlobalState;
use crate::server::{PingResponse, VirtualFileContentParams};

pub(crate) fn handle_virtual_file_content(
    state: &GlobalState,
    params: VirtualFileContentParams,
) -> Option<String> {
    tracing::debug!("Virtual file content requested: {}", params.uri);

    let file_path = graphql_ide::FilePath::new(&params.uri);

    for (_, host) in state.workspace.all_hosts() {
        let analysis = host.snapshot();
        if let Some(content) = analysis.file_content(&file_path) {
            tracing::debug!("Found virtual file content ({} bytes)", content.len());
            return Some(content);
        }
    }

    tracing::debug!("Virtual file not found: {}", params.uri);
    None
}

pub(crate) fn handle_ping() -> PingResponse {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    PingResponse { timestamp }
}

pub(crate) fn handle_trace_capture(
    state: &GlobalState,
    params: crate::trace_capture::TraceCaptureParams,
) -> crate::trace_capture::TraceCaptureResult {
    let Some(ref manager) = state.trace_capture else {
        return crate::trace_capture::TraceCaptureResult {
            status: "error".to_string(),
            path: None,
            message: Some("Trace capture not available (tracing not initialized)".to_string()),
            duration_ms: None,
        };
    };

    match params.action.as_str() {
        "start" => manager.start(),
        "stop" => manager.stop(),
        _ => crate::trace_capture::TraceCaptureResult {
            status: "error".to_string(),
            path: None,
            message: Some(format!("Unknown action: {}", params.action)),
            duration_ms: None,
        },
    }
}
