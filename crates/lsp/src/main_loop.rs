use crossbeam_channel::select;
use lsp_server::{Connection, Message, Notification, Request};

use crate::dispatch::{NotificationDispatcher, RequestDispatcher};
use crate::global_state::{GlobalState, TaskResponse};
use crate::handlers;
use crate::server::{PingRequest, VirtualFileContentRequest};
use crate::trace_capture::TraceCaptureRequest;

pub enum ControlFlow {
    Continue,
    Shutdown,
}

/// Process all currently buffered messages without blocking. Returns
/// `ControlFlow::Shutdown` when the connection has closed.
pub fn tick(connection: &Connection, state: &mut GlobalState) -> ControlFlow {
    use crossbeam_channel::TryRecvError;

    loop {
        match connection.receiver.try_recv() {
            Ok(Message::Request(req)) => {
                if connection.handle_shutdown(&req).expect("shutdown") {
                    return ControlFlow::Shutdown;
                }
                handle_request(state, req);
            }
            Ok(Message::Notification(not)) => handle_notification(state, not),
            Ok(Message::Response(resp)) => tracing::debug!(id = ?resp.id, "client response"),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return ControlFlow::Shutdown,
        }
    }

    while let Ok(task) = state.task_receiver.try_recv() {
        handle_task(state, task.response);
    }

    while let Ok(result) = state.introspection_result_receiver.try_recv() {
        handle_introspection_result(state, result);
    }

    ControlFlow::Continue
}

/// Blocking main loop for the native server. Wakes on any incoming message,
/// then drains all pending messages via `tick`.
pub fn run(connection: &Connection, state: &mut GlobalState) {
    loop {
        select! {
            recv(connection.receiver) -> _ => {}
            recv(state.task_receiver) -> _ => {}
            recv(state.introspection_result_receiver) -> _ => {}
        }
        if matches!(tick(connection, state), ControlFlow::Shutdown) {
            return;
        }
    }
}

fn handle_request(state: &mut GlobalState, req: Request) {
    use lsp_types::request::{
        CodeActionRequest, CodeLensRequest, CodeLensResolve, Completion, DocumentSymbolRequest,
        ExecuteCommand, FoldingRangeRequest, GotoDefinition, HoverRequest, InlayHintRequest,
        PrepareRenameRequest, References, Rename, SelectionRangeRequest, SemanticTokensFullRequest,
        SignatureHelpRequest, WorkspaceSymbolRequest,
    };

    state.in_flight.insert(req.id.clone());

    RequestDispatcher::new(req, state)
        .on_pool::<GotoDefinition, _, _>(
            |p| p.text_document_position_params.text_document.uri.clone(),
            handlers::navigation::handle_goto_definition,
        )
        .on_pool::<HoverRequest, _, _>(
            |p| p.text_document_position_params.text_document.uri.clone(),
            handlers::display::handle_hover,
        )
        .on_pool::<Completion, _, _>(
            |p| p.text_document_position.text_document.uri.clone(),
            handlers::editing::handle_completion,
        )
        .on_pool::<References, _, _>(
            |p| p.text_document_position.text_document.uri.clone(),
            handlers::navigation::handle_references,
        )
        .on_pool::<DocumentSymbolRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::navigation::handle_document_symbol,
        )
        .on_pool::<SemanticTokensFullRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::display::handle_semantic_tokens_full,
        )
        .on_pool::<SelectionRangeRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::display::handle_selection_range,
        )
        .on_pool::<CodeActionRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::editing::handle_code_action,
        )
        .on_pool::<CodeLensRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::display::handle_code_lens,
        )
        .on_pool::<FoldingRangeRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::display::handle_folding_range,
        )
        .on_pool::<InlayHintRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::display::handle_inlay_hint,
        )
        .on_pool::<SignatureHelpRequest, _, _>(
            |p| p.text_document_position_params.text_document.uri.clone(),
            handlers::editing::handle_signature_help,
        )
        .on_pool::<Rename, _, _>(
            |p| p.text_document_position.text_document.uri.clone(),
            handlers::editing::handle_rename,
        )
        .on_pool::<PrepareRenameRequest, _, _>(
            |p| p.text_document.uri.clone(),
            handlers::editing::handle_prepare_rename,
        )
        .on_main::<ExecuteCommand, _>(handlers::editing::handle_execute_command)
        .on_main::<WorkspaceSymbolRequest, _>(handlers::navigation::handle_workspace_symbol)
        .on_main::<VirtualFileContentRequest, _>(handlers::custom::handle_virtual_file_content)
        .on_main::<PingRequest, _>(handlers::custom::handle_ping)
        .on_main::<TraceCaptureRequest, _>(handlers::custom::handle_trace_capture)
        .on_main::<CodeLensResolve, _>(|_state, lens| lens)
        .finish();
}

fn handle_notification(state: &mut GlobalState, not: Notification) {
    use lsp_types::notification::{
        DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
        DidSaveTextDocument,
    };

    if not.method == "$/cancelRequest" {
        if let Ok(params) = serde_json::from_value::<lsp_types::CancelParams>(not.params.clone()) {
            let id = match params.id {
                lsp_types::NumberOrString::Number(n) => lsp_server::RequestId::from(n),
                lsp_types::NumberOrString::String(s) => lsp_server::RequestId::from(s),
            };
            // Only respond if the request is still pending; a response was
            // not yet sent (or the worker beat the cancel notification).
            if state.in_flight.contains(&id) {
                tracing::debug!(?id, "request cancelled by client");
                state.respond(lsp_server::Response::new_err(
                    id,
                    lsp_server::ErrorCode::RequestCanceled as i32,
                    "cancelled".to_owned(),
                ));
            }
        }
        return;
    }

    NotificationDispatcher::new(not, state)
        .on::<DidOpenTextDocument>(handlers::document_sync::handle_did_open)
        .on::<DidChangeTextDocument>(handlers::document_sync::handle_did_change)
        .on::<DidSaveTextDocument>(handlers::document_sync::handle_did_save)
        .on::<DidCloseTextDocument>(handlers::document_sync::handle_did_close)
        .on::<DidChangeWatchedFiles>(handlers::document_sync::handle_did_change_watched_files)
        .finish();
}

fn handle_task(state: &mut GlobalState, response: TaskResponse) {
    match response {
        TaskResponse::Response(resp) => {
            if state.in_flight.contains(&resp.id) {
                state.respond(resp);
            } else {
                tracing::debug!(id = ?resp.id, "dropping response for cancelled request");
            }
        }
        TaskResponse::PublishDiagnosticsForUri {
            uri,
            diagnostics,
            seq,
        } => {
            // Drop stale results: a newer keystroke for this URI bumped the
            // generation while this computation was in flight.
            let latest = state
                .diagnostics_seq
                .get(uri.as_str())
                .copied()
                .unwrap_or(0);
            if seq < latest {
                tracing::debug!(uri = %uri.as_str(), seq, latest, "dropping stale diagnostics");
                return;
            }
            state.publish_diagnostics(uri, diagnostics, None);
        }
        TaskResponse::PublishDiagnosticsBatch(diagnostics) => {
            for (uri, diags) in diagnostics {
                state.publish_diagnostics(uri, diags, None);
            }
        }
    }
}

fn handle_introspection_result(
    state: &mut GlobalState,
    result: crate::global_state::IntrospectionResult,
) {
    match result.result {
        Ok(sdl) => {
            if let Some(host) = state
                .workspace
                .get_host_mut(&result.workspace_uri, &result.project_name)
            {
                let virtual_uri = host.add_introspected_schema(&result.url, &sdl);
                tracing::info!(
                    "Loaded remote schema from {} as {}",
                    result.url,
                    virtual_uri
                );
                state.send_notification::<lsp_types::notification::LogMessage>(
                    lsp_types::LogMessageParams {
                        typ: lsp_types::MessageType::INFO,
                        message: format!("Loaded remote schema from {}", result.url),
                    },
                );
            }
        }
        Err(e) => {
            tracing::error!("Failed to introspect schema from {}: {}", result.url, e);
            state.send_notification::<lsp_types::notification::ShowMessage>(
                lsp_types::ShowMessageParams {
                    typ: lsp_types::MessageType::ERROR,
                    message: format!("Failed to load remote schema from {}: {}", result.url, e),
                },
            );
        }
    }
}
