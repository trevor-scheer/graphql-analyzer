use lsp_server::{ExtractError, Notification, Request};

use crate::global_state::GlobalState;

/// Dispatches a single incoming request to the appropriate handler.
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

/// Dispatches a single incoming notification to the appropriate handler.
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
