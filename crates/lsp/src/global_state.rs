use std::collections::HashSet;

use crossbeam_channel::Sender;
use lsp_server::{Message, RequestId};
use lsp_types::Uri;

use crate::workspace::WorkspaceManager;

pub trait TaskDispatcher: Send + Sync {
    fn execute(&self, work: Box<dyn FnOnce() + Send + 'static>);
}

#[cfg(feature = "native")]
pub struct ThreadPoolDispatcher(threadpool::ThreadPool);

#[cfg(feature = "native")]
impl TaskDispatcher for ThreadPoolDispatcher {
    fn execute(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        self.0.execute(work);
    }
}

#[cfg(feature = "native")]
impl ThreadPoolDispatcher {
    pub fn new(pool: threadpool::ThreadPool) -> Self {
        Self(pool)
    }
}

// Used by the wasm target (no threads) and by tests; native always uses ThreadPoolDispatcher.
#[allow(dead_code)]
pub struct InlineDispatcher;

impl TaskDispatcher for InlineDispatcher {
    fn execute(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        work();
    }
}

/// Owns all mutable server state. Lives exclusively on the main thread.
///
/// Since the main thread is the only writer, no locks are needed for any
/// of these fields. Worker threads receive an immutable `GlobalStateSnapshot`
/// instead.
pub struct GlobalState {
    pub sender: Sender<Message>,
    pub dispatcher: Box<dyn TaskDispatcher>,
    pub workspace: WorkspaceManager,
    pub client_capabilities: Option<lsp_types::ClientCapabilities>,
    pub trace_capture: Option<crate::trace_capture::TraceCaptureManager>,
    pub task_sender: Sender<Task>,
    pub task_receiver: crossbeam_channel::Receiver<Task>,
    pub introspection_request_sender: Sender<IntrospectionRequest>,
    pub introspection_result_receiver: crossbeam_channel::Receiver<IntrospectionResult>,
    pub in_flight: HashSet<RequestId>,
    /// Per-URI generation counter for diagnostics requests. Bumped each time
    /// we spawn a single-URI diagnostics computation; the worker captures the
    /// value and the publish step drops results whose generation no longer
    /// matches (because a newer keystroke has superseded them).
    pub diagnostics_seq: std::collections::HashMap<String, u64>,
}

/// A completed background task ready for the main thread to process.
pub struct Task {
    pub response: TaskResponse,
}

pub enum TaskResponse {
    Response(lsp_server::Response),
    /// Diagnostics for a single URI, tagged with the generation at spawn time.
    /// `handle_task` drops the result if a newer generation has been requested.
    PublishDiagnosticsForUri {
        uri: Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        seq: u64,
    },
    /// Diagnostics for many URIs (e.g. project-wide on save). Always published —
    /// no generation check, so a save+rapid-typing race may briefly publish
    /// stale diagnostics, which the next keystroke corrects.
    PublishDiagnosticsBatch(Vec<(Uri, Vec<lsp_types::Diagnostic>)>),
}

/// Request to fetch a remote schema via introspection (sent to async thread)
pub struct IntrospectionRequest {
    pub workspace_uri: String,
    pub project_name: String,
    pub pending: graphql_ide::PendingIntrospection,
}

/// Result of a remote schema introspection (received from async thread)
pub struct IntrospectionResult {
    pub workspace_uri: String,
    pub project_name: String,
    pub url: String,
    pub result: Result<String, String>,
}

/// Immutable snapshot of server state, passed to worker threads.
pub struct GlobalStateSnapshot {
    pub analysis: graphql_ide::Analysis,
    pub file_path: graphql_ide::FilePath,
}

impl GlobalState {
    pub fn new(
        sender: Sender<Message>,
        dispatcher: Box<dyn TaskDispatcher>,
        introspection_request_sender: Sender<IntrospectionRequest>,
        introspection_result_receiver: crossbeam_channel::Receiver<IntrospectionResult>,
    ) -> Self {
        let (task_sender, task_receiver) = crossbeam_channel::unbounded();

        Self {
            sender,
            dispatcher,
            workspace: WorkspaceManager::new(),
            client_capabilities: None,
            trace_capture: None,
            task_sender,
            task_receiver,
            introspection_request_sender,
            introspection_result_receiver,
            in_flight: HashSet::new(),
            diagnostics_seq: std::collections::HashMap::new(),
        }
    }

    pub fn send_notification<N: lsp_types::notification::Notification>(&self, params: N::Params) {
        let not = lsp_server::Notification::new(
            N::METHOD.to_owned(),
            serde_json::to_value(params).expect("notification params are serializable"),
        );
        self.sender
            .send(Message::Notification(not))
            .expect("client channel open");
    }

    pub fn respond(&mut self, response: lsp_server::Response) {
        self.in_flight.remove(&response.id);
        self.sender
            .send(Message::Response(response))
            .expect("client channel open");
    }

    pub fn publish_diagnostics(
        &self,
        uri: Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        version: Option<i32>,
    ) {
        self.send_notification::<lsp_types::notification::PublishDiagnostics>(
            lsp_types::PublishDiagnosticsParams {
                uri,
                diagnostics,
                version,
            },
        );
    }

    /// Take snapshot for a given URI. Returns None if file not found in any project.
    pub fn snapshot_for_uri(&self, uri: &Uri) -> Option<GlobalStateSnapshot> {
        let (workspace_uri, project_name) = self.workspace.find_workspace_and_project(uri)?;
        let host = self.workspace.get_host(&workspace_uri, &project_name)?;
        let analysis = host.snapshot();
        let file_path = graphql_ide::FilePath::new(uri.to_string());
        Some(GlobalStateSnapshot {
            analysis,
            file_path,
        })
    }

    /// Dispatch a read-only query to the thread pool. The handler returns the
    /// LSP `R::Result` directly (typically `Option<T>`), which is serialized
    /// straight into the JSON-RPC response — `None` becomes `null`.
    pub fn spawn_with_snapshot<F, R>(&mut self, id: lsp_server::RequestId, uri: &Uri, f: F)
    where
        F: FnOnce(GlobalStateSnapshot) -> R + Send + 'static,
        R: serde::Serialize + 'static,
    {
        let Some(snap) = self.snapshot_for_uri(uri) else {
            self.respond(lsp_server::Response::new_ok(id, serde_json::Value::Null));
            return;
        };

        let task_sender = self.task_sender.clone();
        self.dispatcher.execute(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(snap)));
            let response = match result {
                Ok(value) => lsp_server::Response::new_ok(id, value),
                Err(_) => lsp_server::Response::new_err(
                    id,
                    lsp_server::ErrorCode::InternalError as i32,
                    "internal error: handler panicked".to_owned(),
                ),
            };
            let _ = task_sender.send(Task {
                response: TaskResponse::Response(response),
            });
        }));
    }

    /// Spawn a diagnostics computation for a single URI, tagged with the next
    /// generation for that URI. If a later spawn for the same URI bumps the
    /// generation before this one returns, the publish step drops the stale
    /// result.
    pub fn spawn_diagnostics_for_uri<F>(&mut self, uri: Uri, f: F)
    where
        F: FnOnce() -> Vec<lsp_types::Diagnostic> + Send + 'static,
    {
        let seq = self
            .diagnostics_seq
            .entry(uri.to_string())
            .and_modify(|s| *s += 1)
            .or_insert(1);
        let captured_seq = *seq;

        let task_sender = self.task_sender.clone();
        self.dispatcher.execute(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            if let Ok(diagnostics) = result {
                let _ = task_sender.send(Task {
                    response: TaskResponse::PublishDiagnosticsForUri {
                        uri,
                        diagnostics,
                        seq: captured_seq,
                    },
                });
            }
        }));
    }

    /// Spawn a multi-URI diagnostics computation (e.g. project-wide on save).
    /// Results are not generation-checked — see `PublishDiagnosticsBatch`.
    pub fn spawn_diagnostics_batch<F>(&self, f: F)
    where
        F: FnOnce() -> Vec<(Uri, Vec<lsp_types::Diagnostic>)> + Send + 'static,
    {
        let task_sender = self.task_sender.clone();
        self.dispatcher.execute(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            if let Ok(diagnostics) = result {
                let _ = task_sender.send(Task {
                    response: TaskResponse::PublishDiagnosticsBatch(diagnostics),
                });
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use crossbeam_channel::unbounded;

    fn make_state() -> GlobalState {
        let (msg_sender, _msg_receiver) = unbounded();
        let (intro_req_sender, _intro_req_receiver) = unbounded();
        let (_intro_res_sender, intro_res_receiver) = unbounded();
        GlobalState::new(msg_sender, Box::new(InlineDispatcher), intro_req_sender, intro_res_receiver)
    }

    #[test]
    fn diagnostics_seq_bumps_per_uri() {
        let mut state = make_state();
        let a = Uri::from_str("file:///a.graphql").unwrap();
        let b = Uri::from_str("file:///b.graphql").unwrap();

        state.spawn_diagnostics_for_uri(a.clone(), Vec::new);
        assert_eq!(state.diagnostics_seq.get(a.as_str()).copied(), Some(1));

        state.spawn_diagnostics_for_uri(a.clone(), Vec::new);
        assert_eq!(state.diagnostics_seq.get(a.as_str()).copied(), Some(2));

        state.spawn_diagnostics_for_uri(b.clone(), Vec::new);
        assert_eq!(state.diagnostics_seq.get(b.as_str()).copied(), Some(1));
        // Independent counters: bumping b doesn't move a.
        assert_eq!(state.diagnostics_seq.get(a.as_str()).copied(), Some(2));
    }
}

#[cfg(all(test, feature = "native"))]
mod dispatcher_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn inline_dispatcher_runs_work_synchronously() {
        let counter = Arc::new(AtomicUsize::new(0));
        let dispatcher = InlineDispatcher;
        let c = Arc::clone(&counter);
        dispatcher.execute(Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn threadpool_dispatcher_runs_work_eventually() {
        let counter = Arc::new(AtomicUsize::new(0));
        let pool = threadpool::ThreadPool::with_name("test".into(), 2);
        let dispatcher = ThreadPoolDispatcher::new(pool);
        let c = Arc::clone(&counter);
        dispatcher.execute(Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        // Drain the pool by joining; trait doesn't expose join so we sleep+poll.
        for _ in 0..100 {
            if counter.load(Ordering::SeqCst) == 1 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("threadpool dispatcher never ran the task");
    }
}
