use crossbeam_channel::select;
use lsp_server::{Connection, Message, Notification, Request};

use crate::dispatch::{NotificationDispatcher, RequestDispatcher};
use crate::global_state::{GlobalState, TaskResponse};
use crate::handlers;

pub fn main_loop(connection: &Connection, state: &mut GlobalState) {
    loop {
        select! {
            recv(connection.receiver) -> msg => {
                match msg {
                    Ok(Message::Request(req)) => {
                        if connection.handle_shutdown(&req).expect("shutdown") {
                            return;
                        }
                        handle_request(state, req);
                    }
                    Ok(Message::Notification(not)) => {
                        handle_notification(state, not);
                    }
                    Ok(Message::Response(resp)) => {
                        tracing::debug!("got client response: {:?}", resp.id);
                    }
                    Err(_) => {
                        return;
                    }
                }
            }

            recv(state.task_receiver) -> task => {
                if let Ok(task) = task {
                    handle_task(state, task.response);
                }
            }

            recv(state.introspection_result_receiver) -> result => {
                if let Ok(result) = result {
                    handle_introspection_result(state, result);
                }
            }
        }
    }
}

fn handle_request(state: &mut GlobalState, req: Request) {
    if try_dispatch_to_pool(state, &req) {
        return;
    }

    // These handlers run on the main thread with direct state access
    let mut dispatcher = RequestDispatcher::new(req, state);
    dispatcher.finish();
}

/// Try to dispatch a request to the thread pool with a snapshot.
/// Returns true if the request was handled.
fn try_dispatch_to_pool(state: &mut GlobalState, req: &Request) -> bool {
    use lsp_types::request::{
        CodeActionRequest, CodeLensRequest, Completion, DocumentSymbolRequest, FoldingRangeRequest,
        GotoDefinition, HoverRequest, InlayHintRequest, PrepareRenameRequest, References, Rename,
        Request, SelectionRangeRequest, SemanticTokensFullRequest, SignatureHelpRequest,
    };
    #[allow(unused_imports)]
    use lsp_types::{
        CodeActionParams, CodeLensParams, CompletionParams, DocumentSymbolParams,
        FoldingRangeParams, GotoDefinitionParams, HoverParams, InlayHintParams, ReferenceParams,
        RenameParams, SelectionRangeParams, SemanticTokensParams, SignatureHelpParams,
        TextDocumentPositionParams,
    };

    // Dispatch pool-based handlers: extract params, clone URI, spawn on pool

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<GotoDefinitionParams>(GotoDefinition::METHOD)
        {
            let uri = params
                .text_document_position_params
                .text_document
                .uri
                .clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::navigation::handle_goto_definition(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<HoverParams>(HoverRequest::METHOD) {
            let uri = params
                .text_document_position_params
                .text_document
                .uri
                .clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::display::handle_hover(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<CompletionParams>(Completion::METHOD) {
            let uri = params.text_document_position.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::editing::handle_completion(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<ReferenceParams>(References::METHOD) {
            let uri = params.text_document_position.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::navigation::handle_references(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) =
            req_clone.extract::<DocumentSymbolParams>(DocumentSymbolRequest::METHOD)
        {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::navigation::handle_document_symbol(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) =
            req_clone.extract::<SemanticTokensParams>(SemanticTokensFullRequest::METHOD)
        {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::display::handle_semantic_tokens_full(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) =
            req_clone.extract::<SelectionRangeParams>(SelectionRangeRequest::METHOD)
        {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::display::handle_selection_range(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<CodeActionParams>(CodeActionRequest::METHOD) {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::editing::handle_code_action(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<CodeLensParams>(CodeLensRequest::METHOD) {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::display::handle_code_lens(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) =
            req_clone.extract::<FoldingRangeParams>(FoldingRangeRequest::METHOD)
        {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::display::handle_folding_range(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<InlayHintParams>(InlayHintRequest::METHOD) {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::display::handle_inlay_hint(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) =
            req_clone.extract::<SignatureHelpParams>(SignatureHelpRequest::METHOD)
        {
            let uri = params
                .text_document_position_params
                .text_document
                .uri
                .clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::editing::handle_signature_help(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) = req_clone.extract::<RenameParams>(Rename::METHOD) {
            let uri = params.text_document_position.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::editing::handle_rename(snap, params)
            });
            return true;
        }
    }

    {
        let req_clone = req.clone();
        if let Ok((id, params)) =
            req_clone.extract::<TextDocumentPositionParams>(PrepareRenameRequest::METHOD)
        {
            let uri = params.text_document.uri.clone();
            state.spawn_with_snapshot(id, &uri, move |snap| {
                handlers::editing::handle_prepare_rename(snap, params)
            });
            return true;
        }
    }

    // Main-thread handlers (need mutable state or iterate all hosts)
    let req_clone = req.clone();
    match req_clone.extract::<lsp_types::ExecuteCommandParams>(
        <lsp_types::request::ExecuteCommand as lsp_types::request::Request>::METHOD,
    ) {
        Ok((id, params)) => {
            let result = handlers::editing::handle_execute_command(state, params);
            state.respond(lsp_server::Response::new_ok(id, result));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(lsp_server::ExtractError::JsonError { method, error }) => {
            tracing::error!(%method, %error, "invalid request params");
            return true;
        }
    }

    let req_clone = req.clone();
    match req_clone.extract::<lsp_types::WorkspaceSymbolParams>(
        <lsp_types::request::WorkspaceSymbolRequest as lsp_types::request::Request>::METHOD,
    ) {
        Ok((id, params)) => {
            let result = handlers::navigation::handle_workspace_symbol(state, params);
            state.respond(lsp_server::Response::new_ok(id, result));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(lsp_server::ExtractError::JsonError { method, error }) => {
            tracing::error!(%method, %error, "invalid request params");
            return true;
        }
    }

    // Custom methods
    let req_clone = req.clone();
    match req_clone
        .extract::<crate::server::VirtualFileContentParams>("graphql-analyzer/virtualFileContent")
    {
        Ok((id, params)) => {
            let result = handlers::custom::handle_virtual_file_content(state, params);
            state.respond(lsp_server::Response::new_ok(id, result));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(lsp_server::ExtractError::JsonError { method, error }) => {
            tracing::error!(%method, %error, "invalid request params");
            return true;
        }
    }

    let req_clone = req.clone();
    match req_clone.extract::<serde_json::Value>("graphql-analyzer/ping") {
        Ok((id, _)) => {
            let result = handlers::custom::handle_ping();
            state.respond(lsp_server::Response::new_ok(id, result));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(_) => return true,
    }

    let req_clone = req.clone();
    match req_clone
        .extract::<crate::trace_capture::TraceCaptureParams>("graphql-analyzer/traceCapture")
    {
        Ok((id, params)) => {
            let result = handlers::custom::handle_trace_capture(state, params);
            state.respond(lsp_server::Response::new_ok(id, result));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(_) => return true,
    }

    // CodeLensResolve (passthrough)
    let req_clone = req.clone();
    match req_clone.extract::<lsp_types::CodeLens>(
        <lsp_types::request::CodeLensResolve as lsp_types::request::Request>::METHOD,
    ) {
        Ok((id, code_lens)) => {
            state.respond(lsp_server::Response::new_ok(id, code_lens));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(_) => return true,
    }

    false
}

fn handle_notification(state: &mut GlobalState, not: Notification) {
    use lsp_types::notification::{
        DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
        DidSaveTextDocument,
    };

    NotificationDispatcher::new(not, state)
        .on::<DidOpenTextDocument>(handlers::document_sync::handle_did_open)
        .on::<DidChangeTextDocument>(handlers::document_sync::handle_did_change)
        .on::<DidSaveTextDocument>(handlers::document_sync::handle_did_save)
        .on::<DidCloseTextDocument>(handlers::document_sync::handle_did_close)
        .on::<DidChangeWatchedFiles>(handlers::document_sync::handle_did_change_watched_files)
        .finish();
}

fn handle_task(state: &GlobalState, response: TaskResponse) {
    match response {
        TaskResponse::Response(resp) => {
            state.respond(resp);
        }
        TaskResponse::PublishDiagnostics(diagnostics) => {
            for (uri, diags) in diagnostics {
                state.publish_diagnostics(uri, diags, None);
            }
        }
        TaskResponse::Notification(not) => {
            state
                .sender
                .send(Message::Notification(not))
                .expect("client channel open");
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
