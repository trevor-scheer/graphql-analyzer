use lsp_server::{ExtractError, Notification, Request};
use lsp_types::Uri;

use crate::global_state::{GlobalState, GlobalStateSnapshot};

/// Routes one incoming request to the matching handler.
///
/// Methods are tried in chain order; the first matching `on_*` consumes the
/// request. `finish` sends `MethodNotFound` if nothing matched.
pub struct RequestDispatcher<'a> {
    req: Option<Request>,
    state: &'a mut GlobalState,
}

impl<'a> RequestDispatcher<'a> {
    pub fn new(req: Request, state: &'a mut GlobalState) -> Self {
        Self {
            req: Some(req),
            state,
        }
    }

    /// Route a request to the worker pool with a snapshot taken from a URI
    /// extracted from the params. Sends `InvalidParams` on extraction failure
    /// (which also clears the request from `in_flight`).
    pub fn on_pool<R, U, F>(&mut self, extract_uri: U, handler: F) -> &mut Self
    where
        R: lsp_types::request::Request,
        R::Params: serde::de::DeserializeOwned + Send + 'static,
        R::Result: serde::Serialize + 'static,
        U: FnOnce(&R::Params) -> Uri,
        F: FnOnce(GlobalStateSnapshot, R::Params) -> R::Result + Send + 'static,
    {
        let Some(req) = self.req.take() else {
            return self;
        };
        if req.method != R::METHOD {
            self.req = Some(req);
            return self;
        }

        let req_id = req.id.clone();
        match req.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => {
                let uri = extract_uri(&params);
                self.state
                    .spawn_with_snapshot(id, &uri, move |snap| handler(snap, params));
            }
            Err(ExtractError::JsonError { error, .. }) => {
                respond_invalid_params(self.state, req_id, R::METHOD, &error);
            }
            Err(ExtractError::MethodMismatch(_)) => unreachable!("method checked above"),
        }
        self
    }

    /// Route a request to a synchronous handler that runs on the main thread.
    /// The handler returns `R::Result` and the dispatcher serializes it.
    pub fn on_main<R, F>(&mut self, handler: F) -> &mut Self
    where
        R: lsp_types::request::Request,
        R::Params: serde::de::DeserializeOwned,
        R::Result: serde::Serialize,
        F: FnOnce(&mut GlobalState, R::Params) -> R::Result,
    {
        let Some(req) = self.req.take() else {
            return self;
        };
        if req.method != R::METHOD {
            self.req = Some(req);
            return self;
        }

        let req_id = req.id.clone();
        match req.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => {
                let result = handler(self.state, params);
                let value = serde_json::to_value(&result).expect("handler result is serializable");
                self.state.respond(lsp_server::Response::new_ok(id, value));
            }
            Err(ExtractError::JsonError { error, .. }) => {
                respond_invalid_params(self.state, req_id, R::METHOD, &error);
            }
            Err(ExtractError::MethodMismatch(_)) => unreachable!("method checked above"),
        }
        self
    }

    pub fn finish(&mut self) {
        if let Some(req) = self.req.take() {
            tracing::warn!(method = %req.method, "unhandled request");
            let response = lsp_server::Response::new_err(
                req.id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                format!("unhandled request: {}", req.method),
            );
            self.state.respond(response);
        }
    }
}

fn respond_invalid_params(
    state: &mut GlobalState,
    id: lsp_server::RequestId,
    method: &str,
    error: &serde_json::Error,
) {
    tracing::error!(%method, %error, "invalid request params");
    state.respond(lsp_server::Response::new_err(
        id,
        lsp_server::ErrorCode::InvalidParams as i32,
        format!("invalid params: {error}"),
    ));
}

/// Routes one incoming notification to the matching handler.
pub struct NotificationDispatcher<'a> {
    not: Option<Notification>,
    state: &'a mut GlobalState,
}

impl<'a> NotificationDispatcher<'a> {
    pub fn new(not: Notification, state: &'a mut GlobalState) -> Self {
        Self {
            not: Some(not),
            state,
        }
    }

    pub fn on<N>(&mut self, handler: fn(&mut GlobalState, N::Params)) -> &mut Self
    where
        N: lsp_types::notification::Notification,
        N::Params: serde::de::DeserializeOwned + Send + 'static,
    {
        let Some(not) = self.not.take() else {
            return self;
        };

        match not.extract::<N::Params>(N::METHOD) {
            Ok(params) => {
                handler(self.state, params);
            }
            Err(ExtractError::MethodMismatch(not)) => {
                self.not = Some(not);
            }
            Err(ExtractError::JsonError { method, error }) => {
                tracing::error!(%method, %error, "invalid notification params");
            }
        }
        self
    }

    pub fn finish(&mut self) {
        if let Some(not) = self.not.take() {
            if !not.method.starts_with("$/") {
                tracing::warn!(method = %not.method, "unhandled notification");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crossbeam_channel::{unbounded, Receiver};
    use lsp_server::{Message, Request as LspRequest, RequestId};
    use lsp_types::request::Request as _;

    fn make_state() -> (GlobalState, Receiver<Message>) {
        let (msg_sender, msg_receiver) = unbounded::<Message>();
        let (intro_req_sender, _intro_req_receiver) = unbounded();
        let (_intro_res_sender, intro_res_receiver) = unbounded();
        let state = GlobalState::new(
            msg_sender,
            Box::new(crate::global_state::InlineDispatcher),
            intro_req_sender,
            intro_res_receiver,
        );
        (state, msg_receiver)
    }

    #[test]
    fn on_pool_responds_invalid_params_and_clears_in_flight() {
        let (mut state, receiver) = make_state();
        let id = RequestId::from(42);
        state.in_flight.insert(id.clone());

        let req = LspRequest::new(
            id.clone(),
            lsp_types::request::HoverRequest::METHOD.to_owned(),
            serde_json::json!({"this": "is not a hover params object"}),
        );

        RequestDispatcher::new(req, &mut state)
            .on_pool::<lsp_types::request::HoverRequest, _, _>(
                |p| p.text_document_position_params.text_document.uri.clone(),
                |_, _| None,
            )
            .finish();

        assert!(
            !state.in_flight.contains(&id),
            "in_flight should be cleared when params extraction fails"
        );

        let msg = receiver
            .try_recv()
            .expect("an InvalidParams response should be sent");
        let Message::Response(resp) = msg else {
            panic!("expected a Response, got {msg:?}");
        };
        assert_eq!(resp.id, id);
        let err = resp.error.expect("response should carry an error");
        assert_eq!(err.code, lsp_server::ErrorCode::InvalidParams as i32);
    }

    #[test]
    fn on_main_responds_invalid_params_and_clears_in_flight() {
        let (mut state, receiver) = make_state();
        let id = RequestId::from(7);
        state.in_flight.insert(id.clone());

        let req = LspRequest::new(
            id.clone(),
            lsp_types::request::ExecuteCommand::METHOD.to_owned(),
            serde_json::json!("not an object"),
        );

        RequestDispatcher::new(req, &mut state)
            .on_main::<lsp_types::request::ExecuteCommand, _>(|_state, _params| None)
            .finish();

        assert!(!state.in_flight.contains(&id));
        let msg = receiver.try_recv().expect("response was sent");
        let Message::Response(resp) = msg else {
            panic!("expected Response");
        };
        let err = resp.error.expect("error response");
        assert_eq!(err.code, lsp_server::ErrorCode::InvalidParams as i32);
    }

    #[test]
    fn finish_sends_method_not_found_for_unknown_request() {
        let (mut state, receiver) = make_state();
        let id = RequestId::from(13);
        state.in_flight.insert(id.clone());

        let req = LspRequest::new(
            id.clone(),
            "totally/unknown".to_owned(),
            serde_json::Value::Null,
        );

        RequestDispatcher::new(req, &mut state).finish();

        assert!(!state.in_flight.contains(&id));
        let msg = receiver.try_recv().expect("response was sent");
        let Message::Response(resp) = msg else {
            panic!("expected Response");
        };
        let err = resp.error.expect("error response");
        assert_eq!(err.code, lsp_server::ErrorCode::MethodNotFound as i32);
    }
}
