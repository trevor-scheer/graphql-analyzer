use crossbeam_channel::Sender;
use lsp_server::Message;
use lsp_types::Uri;

use crate::workspace::WorkspaceManager;

/// Owns all mutable server state. Lives exclusively on the main thread.
///
/// Since the main thread is the only writer, no locks are needed for any
/// of these fields. Worker threads receive an immutable `GlobalStateSnapshot`
/// instead.
pub struct GlobalState {
    pub sender: Sender<Message>,
    pub threadpool: threadpool::ThreadPool,
    pub workspace: WorkspaceManager,
    pub client_capabilities: Option<lsp_types::ClientCapabilities>,
    pub trace_capture: Option<crate::trace_capture::TraceCaptureManager>,
    pub task_sender: Sender<Task>,
    pub task_receiver: crossbeam_channel::Receiver<Task>,
    pub introspection_request_sender: Sender<IntrospectionRequest>,
    pub introspection_result_receiver: crossbeam_channel::Receiver<IntrospectionResult>,
    #[allow(dead_code)]
    pub shutdown_requested: bool,
}

/// A completed background task ready for the main thread to process.
pub struct Task {
    pub response: TaskResponse,
}

pub enum TaskResponse {
    Response(lsp_server::Response),
    PublishDiagnostics(Vec<(Uri, Vec<lsp_types::Diagnostic>)>),
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
        introspection_request_sender: Sender<IntrospectionRequest>,
        introspection_result_receiver: crossbeam_channel::Receiver<IntrospectionResult>,
    ) -> Self {
        let (task_sender, task_receiver) = crossbeam_channel::unbounded();

        Self {
            sender,
            threadpool: threadpool::ThreadPool::with_name("salsa-worker".into(), num_cpus()),
            workspace: WorkspaceManager::new(),
            client_capabilities: None,
            trace_capture: None,
            task_sender,
            task_receiver,
            introspection_request_sender,
            introspection_result_receiver,
            shutdown_requested: false,
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

    pub fn respond(&self, response: lsp_server::Response) {
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

    /// Dispatch a read-only query to the thread pool.
    pub fn spawn_with_snapshot<F, R>(&self, id: lsp_server::RequestId, uri: &Uri, f: F)
    where
        F: FnOnce(GlobalStateSnapshot) -> Option<R> + Send + 'static,
        R: serde::Serialize + 'static,
    {
        let Some(snap) = self.snapshot_for_uri(uri) else {
            self.respond(lsp_server::Response::new_ok(id, serde_json::Value::Null));
            return;
        };

        let task_sender = self.task_sender.clone();
        self.threadpool.execute(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(snap)));
            let response = match result {
                Ok(Some(value)) => lsp_server::Response::new_ok(id, value),
                Ok(None) => lsp_server::Response::new_ok(id, serde_json::Value::Null),
                Err(_) => lsp_server::Response::new_err(
                    id,
                    lsp_server::ErrorCode::InternalError as i32,
                    "internal error: handler panicked".to_owned(),
                ),
            };
            let _ = task_sender.send(Task {
                response: TaskResponse::Response(response),
            });
        });
    }

    /// Dispatch a diagnostics computation to the thread pool.
    pub fn spawn_diagnostics<F>(&self, f: F)
    where
        F: FnOnce() -> Vec<(Uri, Vec<lsp_types::Diagnostic>)> + Send + 'static,
    {
        let task_sender = self.task_sender.clone();
        self.threadpool.execute(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            if let Ok(diagnostics) = result {
                let _ = task_sender.send(Task {
                    response: TaskResponse::PublishDiagnostics(diagnostics),
                });
            }
        });
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
}
