# Sync LSP Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the tower-lsp async LSP server with a sync `lsp-server` main loop + thread pool architecture, eliminating the async/sync boundary that has caused deadlocks.

**Architecture:** Single-threaded main loop using `crossbeam_channel::select!` owns all mutable state. Read-only Salsa queries run on a `threadpool` worker pool. A dedicated thread with a Tokio runtime handles async HTTP introspection. The `graphql-ide` layer is already fully sync and unchanged.

**Tech Stack:** `lsp-server` 0.7 (protocol framing), `crossbeam-channel` (event loop), `threadpool` (Salsa query workers), `lsp-types` 0.97 (already a transitive dep via tower-lsp, now direct), `tokio` retained minimally for introspection thread only.

---

## File Structure

### New Files
| File | Responsibility |
|---|---|
| `crates/lsp/src/main_loop.rs` | Main event loop, `Event` enum, dispatch logic |
| `crates/lsp/src/global_state.rs` | `GlobalState` (mutable owner), `GlobalStateSnapshot` (immutable reader) |
| `crates/lsp/src/dispatch.rs` | `RequestDispatcher` and `NotificationDispatcher` chain helpers |

### Modified Files
| File | Change |
|---|---|
| `Cargo.toml` (workspace root) | Add `lsp-server`, `crossbeam-channel`, `threadpool` workspace deps |
| `crates/lsp/Cargo.toml` | Swap `tower-lsp-server` for `lsp-server` + `crossbeam-channel` + `threadpool`; keep `tokio` with reduced features for introspection |
| `crates/lsp/src/lib.rs` | Replace async `run_server()` with sync version; keep tracing init |
| `crates/lsp/src/main.rs` | Remove `#[tokio::main]`, use plain `fn main()` |
| `crates/lsp/src/server.rs` | Remove `GraphQLLanguageServer`, `LanguageServer` impl, `with_analysis`/`blocking` helpers. Keep `StatusNotification`, `PingResponse`, `VirtualFileContentParams`, `validation_errors_to_diagnostics`, `describe_join_error` (adapted for `std::thread` panics) |
| `crates/lsp/src/workspace.rs` | Remove `ProjectHost` async wrapper, replace `DashMap` with `HashMap`, make all methods sync |
| `crates/lsp/src/handlers/navigation.rs` | Sync signatures, take `&GlobalStateSnapshot` |
| `crates/lsp/src/handlers/display.rs` | Sync signatures, take `&GlobalStateSnapshot` |
| `crates/lsp/src/handlers/editing.rs` | Sync signatures, take `&GlobalStateSnapshot` or `&mut GlobalState` |
| `crates/lsp/src/handlers/document_sync.rs` | Sync signatures, take `&mut GlobalState` (main thread mutations) |
| `crates/lsp/src/conversions.rs` | Update imports from `tower_lsp_server::ls_types` to `lsp_types` directly |
| `crates/lsp/src/trace_capture.rs` | No logic changes; update if any tower-lsp types were used (they aren't) |

### Deleted Code
- `ProjectHost` struct and all its methods (replaced by direct `AnalysisHost` ownership)
- `LanguageServer` trait impl (replaced by dispatch chain)
- `with_analysis()`, `blocking()` helpers (no longer needed)
- `load_workspaces_background()` free function (replaced by `GlobalState::load_workspaces_on_pool`)
- `describe_join_error()` (no more `JoinError` — panics caught by `thread::catch_unwind` or pool)
- All `#[allow(clippy::unused_async)]` annotations
- Deadlock regression tests in `workspace.rs` (the conditions they test are structurally impossible)

---

## Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root, lines 32-84)
- Modify: `crates/lsp/Cargo.toml` (lines 20-60)

- [ ] **Step 1: Add workspace dependencies**

In the workspace root `Cargo.toml`, add these to `[workspace.dependencies]`:

```toml
# LSP (sync)
lsp-server = "0.7"
lsp-types = "0.97"
crossbeam-channel = "0.5"
threadpool = "1.8"
```

- [ ] **Step 2: Update `crates/lsp/Cargo.toml`**

Replace `tower-lsp-server` and reduce `tokio`:

```toml
[dependencies]
# ...existing internal deps unchanged...

# LSP (sync server + types)
lsp-server = { workspace = true }
lsp-types = { workspace = true }
crossbeam-channel = { workspace = true }
threadpool = { workspace = true }

# Async - retained only for introspection HTTP calls
tokio = { workspace = true, features = ["rt", "sync"] }

# ...rest unchanged...
```

Remove `tower-lsp-server` from dependencies entirely.

Remove `tokio` from `[dev-dependencies]` (no longer needed for tests).

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p graphql-lsp 2>&1 | head -5`
Expected: Compile errors from removed `tower-lsp-server` imports (this is expected, we'll fix in subsequent tasks)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/lsp/Cargo.toml
git commit -m "swap tower-lsp-server for lsp-server + crossbeam-channel + threadpool"
```

---

## Task 2: Create `GlobalState` and `GlobalStateSnapshot`

**Files:**
- Create: `crates/lsp/src/global_state.rs`

This task creates the two core types that replace `GraphQLLanguageServer`. `GlobalState` lives on the main thread and owns all mutable state. `GlobalStateSnapshot` is a cheap immutable view passed to worker threads.

- [ ] **Step 1: Create `global_state.rs`**

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::Sender;
use lsp_server::Message;
use lsp_types::{ClientCapabilities, Uri};

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
    pub client_capabilities: Option<ClientCapabilities>,
    pub trace_capture: Option<crate::trace_capture::TraceCaptureManager>,
    /// Channel for receiving completed task results back on the main thread
    pub task_sender: Sender<Task>,
    pub task_receiver: crossbeam_channel::Receiver<Task>,
    /// Send introspection requests to the async runtime thread
    pub introspection_request_sender: Sender<IntrospectionRequest>,
    /// Receive introspection results from the async runtime thread
    pub introspection_result_receiver: crossbeam_channel::Receiver<IntrospectionResult>,
    /// Shutdown flag
    pub shutdown_requested: bool,
}

/// A completed background task ready for the main thread to process.
pub struct Task {
    /// The original request ID to respond to (None for fire-and-forget tasks
    /// like diagnostics publishing)
    pub response: TaskResponse,
}

pub enum TaskResponse {
    /// Send an LSP response for a request
    Response(lsp_server::Response),
    /// Publish diagnostics (no request ID)
    PublishDiagnostics(Vec<(Uri, Vec<lsp_types::Diagnostic>)>),
    /// Log a message to the client
    LogMessage(lsp_types::MessageType, String),
    /// Show a message to the client
    ShowMessage(lsp_types::MessageType, String),
    /// Send a custom notification
    Notification(lsp_server::Notification),
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
///
/// Contains only what a request handler needs to compute its result.
/// No locks, no mutability, no channel back to the main thread
/// (the thread pool's completion channel handles that).
pub struct GlobalStateSnapshot {
    pub analysis: graphql_ide::Analysis,
    pub file_path: graphql_ide::FilePath,
}

impl GlobalState {
    pub fn new(sender: Sender<Message>) -> Self {
        let (task_sender, task_receiver) = crossbeam_channel::unbounded();
        let (introspection_request_sender, introspection_request_receiver) =
            crossbeam_channel::unbounded();
        let (introspection_result_sender, introspection_result_receiver) =
            crossbeam_channel::unbounded();

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
            // Store these for passing to the introspection thread at startup:
            // introspection_request_receiver and introspection_result_sender
            // are moved into spawn_introspection_thread() in lib.rs
            shutdown_requested: false,
        }
    }

    /// Send a notification to the client (convenience method for main thread)
    pub fn send_notification<N: lsp_types::notification::Notification>(
        &self,
        params: N::Params,
    ) {
        let not = lsp_server::Notification::new(
            N::METHOD.to_owned(),
            serde_json::to_value(params).expect("notification params are serializable"),
        );
        self.sender
            .send(Message::Notification(not))
            .expect("client channel open");
    }

    /// Send a response to a request (convenience method for main thread)
    pub fn respond(&self, response: lsp_server::Response) {
        self.sender
            .send(Message::Response(response))
            .expect("client channel open");
    }

    /// Publish diagnostics for a single file
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
        let (workspace_uri, project_name) =
            self.workspace.find_workspace_and_project(uri)?;
        let host = self.workspace.get_host(&workspace_uri, &project_name)?;
        let analysis = host.snapshot();
        let file_path = graphql_ide::FilePath::new(uri.to_string());
        Some(GlobalStateSnapshot {
            analysis,
            file_path,
        })
    }

    /// Dispatch a read-only query to the thread pool.
    ///
    /// Takes a snapshot, spawns the closure on a worker thread, and sends the
    /// response back through the task channel. The main loop picks it up
    /// and forwards it to the client.
    pub fn spawn_with_snapshot<F, R>(
        &self,
        id: lsp_server::RequestId,
        uri: &Uri,
        f: F,
    ) where
        F: FnOnce(GlobalStateSnapshot) -> Option<R> + Send + 'static,
        R: serde::Serialize + 'static,
    {
        let snap = match self.snapshot_for_uri(uri) {
            Some(snap) => snap,
            None => {
                self.respond(lsp_server::Response::new_ok(
                    id,
                    serde_json::Value::Null,
                ));
                return;
            }
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

    /// Dispatch a diagnostics computation to the thread pool (no request ID).
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
```

- [ ] **Step 2: Register the module**

In `crates/lsp/src/lib.rs`, add (alongside existing `mod` declarations):
```rust
mod global_state;
```

- [ ] **Step 3: Commit**

```bash
git add crates/lsp/src/global_state.rs crates/lsp/src/lib.rs
git commit -m "add GlobalState and GlobalStateSnapshot types"
```

---

## Task 3: Create Request/Notification Dispatch Helpers

**Files:**
- Create: `crates/lsp/src/dispatch.rs`

Provides ergonomic dispatch chains for routing incoming LSP messages to handlers, modeled after rust-analyzer's `RequestDispatcher`/`NotificationDispatcher`.

- [ ] **Step 1: Create `dispatch.rs`**

```rust
use lsp_server::{ExtractError, Notification, Request, RequestId};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;

use crate::global_state::GlobalState;

/// Dispatches a single incoming request to the appropriate handler.
///
/// Usage:
/// ```ignore
/// RequestDispatcher::new(req, state)
///     .on::<GotoDefinition>(handlers::handle_goto_definition)
///     .on::<Hover>(handlers::handle_hover)
///     .finish();
/// ```
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

    /// Handle a request by spawning the handler on the thread pool.
    ///
    /// The handler receives a `GlobalStateSnapshot` and returns a serializable
    /// result. The response is sent back to the main loop via the task channel.
    pub fn on<R>(
        &mut self,
        handler: fn(&GlobalState, R::Params) -> Option<R::Result>,
    ) -> &mut Self
    where
        R: lsp_types::request::Request,
        R::Params: serde::de::DeserializeOwned + Send + 'static,
        R::Result: serde::Serialize + Send + 'static,
    {
        let req = match self.req.take() {
            Some(req) => req,
            None => return self,
        };

        match req.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => {
                let result = handler(self.state, params);
                let response = lsp_server::Response::new_ok(id, result);
                self.state.respond(response);
            }
            Err(ExtractError::MethodMismatch(req)) => {
                self.req = Some(req);
            }
            Err(ExtractError::JsonError { method, error }) => {
                tracing::error!(%method, %error, "invalid request params");
            }
        }
        self
    }

    /// Handle a request that needs mutable access to GlobalState (runs on main thread).
    pub fn on_mut<R>(
        &mut self,
        handler: fn(&mut GlobalState, R::Params) -> Option<R::Result>,
    ) -> &mut Self
    where
        R: lsp_types::request::Request,
        R::Params: serde::de::DeserializeOwned + Send + 'static,
        R::Result: serde::Serialize + Send + 'static,
    {
        let req = match self.req.take() {
            Some(req) => req,
            None => return self,
        };

        match req.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => {
                let result = handler(self.state, params);
                let response = lsp_server::Response::new_ok(id, result);
                self.state.respond(response);
            }
            Err(ExtractError::MethodMismatch(req)) => {
                self.req = Some(req);
            }
            Err(ExtractError::JsonError { method, error }) => {
                tracing::error!(%method, %error, "invalid request params");
            }
        }
        self
    }

    // Note: Pool dispatch is handled by the `dispatch_pool!` macro in
    // main_loop.rs, not here. The macro knows how to extract the URI from
    // each request type's params to construct the snapshot.

    /// Log a warning for unhandled requests and send MethodNotFound.
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

use crate::global_state::GlobalStateSnapshot;

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

    /// Handle a notification on the main thread (all notifications mutate state).
    pub fn on<N>(
        &mut self,
        handler: fn(&mut GlobalState, N::Params),
    ) -> &mut Self
    where
        N: lsp_types::notification::Notification,
        N::Params: serde::de::DeserializeOwned + Send + 'static,
    {
        let not = match self.not.take() {
            Some(not) => not,
            None => return self,
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
```

- [ ] **Step 2: Register the module**

In `crates/lsp/src/lib.rs`, add:
```rust
mod dispatch;
```

- [ ] **Step 3: Commit**

```bash
git add crates/lsp/src/dispatch.rs crates/lsp/src/lib.rs
git commit -m "add RequestDispatcher and NotificationDispatcher"
```

---

## Task 4: Migrate `WorkspaceManager` to Sync

**Files:**
- Modify: `crates/lsp/src/workspace.rs`

Remove the `ProjectHost` async wrapper entirely. Replace `DashMap` with `HashMap`. All methods become plain `&self`/`&mut self`. The `AnalysisHost` instances are now directly owned by the `WorkspaceManager` without any `Mutex` — the main thread is the sole writer.

- [ ] **Step 1: Replace imports and remove `ProjectHost`**

Remove these imports:
```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tower_lsp_server::ls_types as lsp_types;
use crate::server::describe_join_error;
```

Replace with:
```rust
use lsp_types::Uri;
```

Delete the entire `ProjectHost` struct, its `impl` block, and its `Default` impl (lines 30-148).

Delete the `LOCK_TIMEOUT` constant.

- [ ] **Step 2: Replace `DashMap` with `HashMap` in `WorkspaceManager`**

Replace all `DashMap<K, V>` fields with `HashMap<K, V>`, and change the `hosts` field to store `AnalysisHost` directly:

```rust
use std::collections::HashMap;
use graphql_ide::AnalysisHost;

pub struct WorkspaceManager {
    pub init_workspace_folders: HashMap<String, PathBuf>,
    pub workspace_roots: HashMap<String, PathBuf>,
    pub config_paths: HashMap<String, PathBuf>,
    pub configs: HashMap<String, graphql_config::GraphQLConfig>,
    hosts: HashMap<(String, String), AnalysisHost>,
    pub document_versions: HashMap<String, i32>,
    pub document_contents: HashMap<String, String>,
    pub file_to_project: HashMap<String, (String, String)>,
    pub resolved_schema_paths: HashMap<(String, String), PathBuf>,
}
```

- [ ] **Step 3: Update `WorkspaceManager::new()`**

```rust
impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            init_workspace_folders: HashMap::new(),
            workspace_roots: HashMap::new(),
            config_paths: HashMap::new(),
            configs: HashMap::new(),
            hosts: HashMap::new(),
            document_versions: HashMap::new(),
            document_contents: HashMap::new(),
            file_to_project: HashMap::new(),
            resolved_schema_paths: HashMap::new(),
        }
    }
}
```

- [ ] **Step 4: Update host access methods**

Replace `ProjectHost`-returning methods with direct `AnalysisHost` access:

```rust
/// Get or create an `AnalysisHost` for a workspace/project
pub fn get_or_create_host(
    &mut self,
    workspace_uri: &str,
    project_name: &str,
) -> &mut AnalysisHost {
    self.hosts
        .entry((workspace_uri.to_string(), project_name.to_string()))
        .or_insert_with(AnalysisHost::new)
}

/// Get an existing `AnalysisHost` reference
pub fn get_host(
    &self,
    workspace_uri: &str,
    project_name: &str,
) -> Option<&AnalysisHost> {
    self.hosts
        .get(&(workspace_uri.to_string(), project_name.to_string()))
}

/// Get a mutable reference to an existing host
pub fn get_host_mut(
    &mut self,
    workspace_uri: &str,
    project_name: &str,
) -> Option<&mut AnalysisHost> {
    self.hosts
        .get_mut(&(workspace_uri.to_string(), project_name.to_string()))
}

/// Return all (workspace_uri, project_name, host) triples
pub fn all_hosts(&self) -> impl Iterator<Item = (&(String, String), &AnalysisHost)> {
    self.hosts.iter()
}

/// Return hosts for a given workspace
pub fn projects_for_workspace(
    &self,
    workspace_uri: &str,
) -> Vec<(&str, &AnalysisHost)> {
    self.hosts
        .iter()
        .filter(|((ws, _), _)| ws == workspace_uri)
        .map(|((_, name), host)| (name.as_str(), host))
        .collect()
}
```

- [ ] **Step 5: Update `find_workspace_and_project` and other methods**

These are already sync — just remove `DashMap`-specific access patterns (`.get()` returning `Ref`, `.iter()` holding shard locks). With `HashMap` these are normal borrows:

```rust
pub fn find_workspace_and_project(&self, document_uri: &Uri) -> Option<(String, String)> {
    let uri_string = document_uri.to_string();

    if let Some(entry) = self.file_to_project.get(&uri_string) {
        return Some(entry.clone());
    }

    if !uri_string.starts_with("file://") {
        return None;
    }

    let doc_path = document_uri.to_file_path()?;
    for (workspace_uri, workspace_path) in &self.workspace_roots {
        if doc_path.as_ref().starts_with(workspace_path.as_path()) {
            if let Some(config) = self.configs.get(workspace_uri.as_str()) {
                if let Some(project_name) =
                    config.find_project_for_document(&doc_path, workspace_path)
                {
                    return Some((workspace_uri.clone(), project_name.to_string()));
                }
            }
            return None;
        }
    }

    None
}
```

Update `clear_workspace` to use `HashMap::retain`:

```rust
pub fn clear_workspace(&mut self, workspace_uri: &str) {
    self.hosts.retain(|(ws, _), _| ws != workspace_uri);
    self.file_to_project.retain(|_, (ws, _)| ws != workspace_uri);
    self.configs.remove(workspace_uri);
}
```

Remove `find_workspace_and_project_async` and `find_host_for_virtual_file` (async methods no longer needed — virtual file lookup can iterate hosts synchronously since there's no lock).

- [ ] **Step 6: Update `apply_content_change`** — no changes needed, it's already sync.

- [ ] **Step 7: Update tests**

Remove the two async deadlock regression tests (`test_add_file_and_snapshot_does_not_block_async_runtime` and `test_concurrent_snapshot_lookups_during_writer`). These tested the async/sync boundary that no longer exists.

Update remaining tests to use `HashMap` API instead of `DashMap`:

```rust
#[test]
fn test_workspace_manager_creation() {
    let manager = WorkspaceManager::new();
    assert!(manager.workspace_roots.is_empty());
    assert!(manager.get_host("nonexistent", "nonexistent").is_none());
}

#[test]
fn test_get_or_create_host() {
    let mut manager = WorkspaceManager::new();
    let _host1 = manager.get_or_create_host("workspace1", "project1");
    let _host2 = manager.get_or_create_host("workspace1", "project2");
    // Different projects should get different hosts
    assert!(manager.get_host("workspace1", "project1").is_some());
    assert!(manager.get_host("workspace1", "project2").is_some());
}
```

The `ptr_eq` test doesn't apply anymore since hosts are directly owned (not `Arc`-wrapped).

- [ ] **Step 8: Commit**

```bash
git add crates/lsp/src/workspace.rs
git commit -m "migrate WorkspaceManager from DashMap + async Mutex to sync HashMap"
```

---

## Task 5: Migrate Notification Handlers (Document Sync)

**Files:**
- Modify: `crates/lsp/src/handlers/document_sync.rs`

All document sync handlers become sync functions that take `&mut GlobalState`. They mutate the workspace directly on the main thread (no locks needed) and dispatch diagnostics computation to the thread pool.

- [ ] **Step 1: Rewrite `handle_did_open`**

```rust
use crate::conversions::convert_ide_diagnostic;
use crate::global_state::GlobalState;
use graphql_ide::{DocumentKind, Language};
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    FileChangeType, Uri,
};
use std::path::Path;
use std::str::FromStr;

pub(crate) fn handle_did_open(state: &mut GlobalState, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let content = params.text_document.text;
    let version = params.text_document.version;

    tracing::info!("File opened: {}", uri.path());

    let uri_string = uri.to_string();
    state
        .workspace
        .document_versions
        .insert(uri_string.clone(), version);
    state
        .workspace
        .document_contents
        .insert(uri_string.clone(), content.clone());

    let Some((workspace_uri, project_name)) =
        state.workspace.find_workspace_and_project(&uri)
    else {
        tracing::debug!("File not covered by any project config, ignoring");
        return;
    };

    state.workspace.file_to_project.insert(
        uri_string,
        (workspace_uri.clone(), project_name.clone()),
    );

    let language =
        Language::from_path(Path::new(uri.path().as_str())).unwrap_or(Language::GraphQL);

    let document_kind = state
        .workspace
        .get_file_type(&uri, &workspace_uri, &project_name)
        .map_or(DocumentKind::Executable, |ft| match ft {
            graphql_config::FileType::Schema => DocumentKind::Schema,
            graphql_config::FileType::Document => DocumentKind::Executable,
        });

    let file_path = graphql_ide::FilePath::new(uri.to_string());
    let host = state
        .workspace
        .get_or_create_host(&workspace_uri, &project_name);
    let (is_new, snapshot) =
        host.update_file_and_snapshot(&file_path, &content, language, document_kind);

    if is_new {
        let diagnostics: Vec<lsp_types::Diagnostic> = snapshot
            .all_diagnostics_for_file(&file_path)
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();
        state.publish_diagnostics(uri, diagnostics, None);
    }
}
```

- [ ] **Step 2: Rewrite `handle_did_change`**

```rust
pub(crate) fn handle_did_change(state: &mut GlobalState, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri;
    let version = params.text_document.version;

    tracing::info!("File changed: {} (v{})", uri.path(), version);

    let uri_string = uri.to_string();
    if let Some(&current_version) = state.workspace.document_versions.get(&uri_string) {
        if version <= current_version {
            tracing::warn!(
                "Ignoring stale document update: version {} <= current {}",
                version,
                current_version
            );
            return;
        }
    }
    state
        .workspace
        .document_versions
        .insert(uri_string.clone(), version);

    let current_content = {
        let mut current_content = state
            .workspace
            .document_contents
            .get(&uri_string)
            .cloned()
            .unwrap_or_default();

        for change in &params.content_changes {
            current_content = crate::workspace::apply_content_change(&current_content, change);
        }

        state
            .workspace
            .document_contents
            .insert(uri_string.clone(), current_content.clone());

        current_content
    };

    let Some((workspace_uri, project_name)) =
        state.workspace.find_workspace_and_project(&uri)
    else {
        return;
    };

    let language =
        Language::from_path(Path::new(uri.path().as_str())).unwrap_or(Language::GraphQL);

    let document_kind = state
        .workspace
        .get_file_type(&uri, &workspace_uri, &project_name)
        .map_or(DocumentKind::Executable, |ft| match ft {
            graphql_config::FileType::Schema => DocumentKind::Schema,
            graphql_config::FileType::Document => DocumentKind::Executable,
        });

    let file_path = graphql_ide::FilePath::new(uri.to_string());
    let host = state
        .workspace
        .get_or_create_host(&workspace_uri, &project_name);
    let (_is_new, snapshot) =
        host.update_file_and_snapshot(&file_path, &current_content, language, document_kind);

    // Spawn diagnostics computation to thread pool
    let file_path_clone = graphql_ide::FilePath::new(uri.as_str());
    let uri_clone = uri.clone();
    state.spawn_diagnostics(move || {
        let diagnostics: Vec<lsp_types::Diagnostic> = snapshot
            .diagnostics(&file_path_clone)
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();
        vec![(uri_clone, diagnostics)]
    });
}
```

- [ ] **Step 3: Rewrite `handle_did_save`**

```rust
pub(crate) fn handle_did_save(state: &mut GlobalState, params: DidSaveTextDocumentParams) {
    if let Some(ref manager) = state.trace_capture {
        if manager.check_auto_stop() {
            state.send_notification::<lsp_types::notification::LogMessage>(
                lsp_types::LogMessageParams {
                    typ: lsp_types::MessageType::INFO,
                    message: "Trace capture auto-stopped after 60s timeout".to_owned(),
                },
            );
        }
    }

    let uri = params.text_document.uri;
    tracing::info!("File saved: {}", uri.path());

    let Some((workspace_uri, project_name)) =
        state.workspace.find_workspace_and_project(&uri)
    else {
        return;
    };

    let Some(host) = state.workspace.get_host(&workspace_uri, &project_name) else {
        return;
    };

    let snapshot = host.snapshot();
    let changed_file = graphql_ide::FilePath::new(uri.as_str());

    state.spawn_diagnostics(move || {
        let all_diagnostics = snapshot.all_diagnostics_for_change(&changed_file);
        all_diagnostics
            .into_iter()
            .filter_map(|(file_path, diags)| {
                let file_uri = Uri::from_str(file_path.as_str()).ok()?;
                let lsp_diagnostics = diags.into_iter().map(convert_ide_diagnostic).collect();
                Some((file_uri, lsp_diagnostics))
            })
            .collect()
    });
}
```

- [ ] **Step 4: Rewrite `handle_did_close` and `handle_did_change_watched_files`**

```rust
pub(crate) fn handle_did_close(state: &mut GlobalState, params: DidCloseTextDocumentParams) {
    tracing::info!("File closed: {}", params.text_document.uri.path());
    let uri_string = params.text_document.uri.to_string();
    state.workspace.document_versions.remove(&uri_string);
    state.workspace.document_contents.remove(&uri_string);
}

pub(crate) fn handle_did_change_watched_files(
    state: &mut GlobalState,
    params: DidChangeWatchedFilesParams,
) {
    tracing::debug!("Watched files changed: {} file(s)", params.changes.len());

    for change in params.changes {
        let uri = change.uri;
        tracing::debug!("File changed: {} (type: {:?})", uri.path(), change.typ);

        let Some(config_path) = uri.to_file_path() else {
            tracing::warn!("Failed to convert URI to file path: {:?}", uri);
            continue;
        };

        // Check if changed file is a config file
        let workspace_uri: Option<String> = state
            .workspace
            .config_paths
            .iter()
            .find(|(_, path)| **path == config_path)
            .map(|(ws_uri, _)| ws_uri.clone());

        if let Some(workspace_uri) = workspace_uri {
            match change.typ {
                FileChangeType::CREATED | FileChangeType::CHANGED => {
                    tracing::info!("Config file changed for workspace: {}", workspace_uri);
                    reload_workspace_config(state, &workspace_uri);
                }
                FileChangeType::DELETED => {
                    tracing::warn!("Config file deleted for workspace: {}", workspace_uri);
                    state.send_notification::<lsp_types::notification::ShowMessage>(
                        lsp_types::ShowMessageParams {
                            typ: lsp_types::MessageType::WARNING,
                            message: "GraphQL config file was deleted".to_owned(),
                        },
                    );
                }
                _ => {}
            }
            continue;
        }

        // Check if changed file is a resolved schema
        let resolved_match: Option<(String, String)> = state
            .workspace
            .resolved_schema_paths
            .iter()
            .find(|(_, path)| **path == config_path)
            .map(|((ws, proj), _)| (ws.clone(), proj.clone()));

        if let Some((ws_uri, proj_name)) = resolved_match {
            if matches!(
                change.typ,
                FileChangeType::CREATED | FileChangeType::CHANGED
            ) {
                reload_resolved_schema(state, &ws_uri, &proj_name, &config_path);
            }
        }
    }
}
```

- [ ] **Step 5: Port workspace loading functions to sync**

Create a new file `crates/lsp/src/loading.rs` with the following free functions, all taking `&mut GlobalState`:

| Old function (server.rs) | New function (loading.rs) | Key changes |
|---|---|---|
| `load_workspace_config()` (line 810) | `load_workspace_config(state, ws_uri, ws_path)` | Remove `.await`; replace `self.client.show_message_request().await` with `state.send_notification::<ShowMessage>(...)` (fire-and-forget, no response); replace `self.client.publish_diagnostics().await` with `state.publish_diagnostics()` |
| `load_all_project_files()` (line 1083) | `load_all_project_files(state, ws_uri, ws_path, config, config_path)` | Remove `.await`; call `host.set_extract_config()` / `host.set_lint_config()` directly (no lock); call `host.load_schemas_from_config()` directly; send introspection requests via `state.introspection_request_sender.send(...)` instead of `client.execute().await`; diagnostics via `state.publish_diagnostics()` |
| `reload_workspace_config()` (line 1446) | `reload_workspace_config(state, ws_uri)` | Same pattern; calls `load_workspace_config` |
| `reload_resolved_schema()` (line 1383) | `reload_resolved_schema(state, ws_uri, proj, path)` | Direct `host.add_file()` call, then `host.snapshot()` + diagnostics |
| `create_default_config()` (line 967) | `create_default_config(state, ws_path)` | Replace `self.client.show_message().await` with `state.send_notification::<ShowMessage>()` |
| `fetch_remote_schemas()` (line 1011) | Not needed — replaced by `state.introspection_request_sender.send()` in `load_all_project_files` |

The `window/showMessageRequest` pattern (which awaits a client response) becomes a one-way `window/showMessage` for now. The interactive "Create Config" / "Open Config" dialogs lose their interactivity. This is an acceptable tradeoff — the config still loads, the user just doesn't get the clickable button.

Register the module: add `pub(crate) mod loading;` to `lib.rs`.

Update `handle_did_change_watched_files` to call `loading::reload_workspace_config` and `loading::reload_resolved_schema` instead of `server.reload_workspace_config().await`.

- [ ] **Step 6: Commit**

```bash
git add crates/lsp/src/handlers/document_sync.rs
git commit -m "migrate document sync handlers to sync"
```

---

## Task 6: Migrate Request Handlers

**Files:**
- Modify: `crates/lsp/src/handlers/navigation.rs`
- Modify: `crates/lsp/src/handlers/display.rs`
- Modify: `crates/lsp/src/handlers/editing.rs`
- Modify: `crates/lsp/src/conversions.rs`

All request handlers become sync functions. They receive `GlobalStateSnapshot` (or `&GlobalState` for handlers that need workspace-wide iteration). The conversion is mechanical: remove `async`, remove `.await`, remove `server.with_analysis()` wrapper, call the analysis method directly.

- [ ] **Step 1: Update `conversions.rs` imports**

Replace:
```rust
use tower_lsp_server::ls_types as lsp_types;
```
With nothing — `lsp_types` is now a direct dependency, so all existing `lsp_types::` references work as-is. Remove the import alias line entirely.

- [ ] **Step 2: Rewrite `navigation.rs`**

```rust
use crate::conversions::{
    convert_ide_document_symbol, convert_ide_location, convert_ide_workspace_symbol,
    convert_lsp_position,
};
use crate::global_state::{GlobalState, GlobalStateSnapshot};
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Location, ReferenceParams,
};

pub(crate) fn handle_goto_definition(
    snap: GlobalStateSnapshot,
    params: GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let position = convert_lsp_position(params.text_document_position_params.position);
    let locations = snap.analysis.goto_definition(&snap.file_path, position)?;
    let lsp_locations: Vec<Location> = locations.iter().map(convert_ide_location).collect();
    if lsp_locations.is_empty() {
        None
    } else {
        Some(GotoDefinitionResponse::Array(lsp_locations))
    }
}

pub(crate) fn handle_references(
    snap: GlobalStateSnapshot,
    params: ReferenceParams,
) -> Option<Vec<Location>> {
    let position = convert_lsp_position(params.text_document_position.position);
    let include_declaration = params.context.include_declaration;
    let locations =
        snap.analysis
            .find_references(&snap.file_path, position, include_declaration)?;
    let lsp_locations: Vec<Location> = locations
        .into_iter()
        .map(|loc| convert_ide_location(&loc))
        .collect();
    if lsp_locations.is_empty() {
        None
    } else {
        Some(lsp_locations)
    }
}

pub(crate) fn handle_document_symbol(
    snap: GlobalStateSnapshot,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let symbols = snap.analysis.document_symbols(&snap.file_path);
    if symbols.is_empty() {
        return None;
    }
    let lsp_symbols: Vec<lsp_types::DocumentSymbol> = symbols
        .into_iter()
        .map(convert_ide_document_symbol)
        .collect();
    Some(DocumentSymbolResponse::Nested(lsp_symbols))
}

pub(crate) fn handle_workspace_symbol(
    state: &GlobalState,
    params: lsp_types::WorkspaceSymbolParams,
) -> Option<lsp_types::WorkspaceSymbolResponse> {
    let mut all_symbols = Vec::new();

    for (_, host) in state.workspace.all_hosts() {
        let analysis = host.snapshot();
        let symbols = analysis.workspace_symbols(&params.query);
        for symbol in symbols {
            all_symbols.push(convert_ide_workspace_symbol(symbol));
        }
    }

    if all_symbols.is_empty() {
        return None;
    }
    Some(lsp_types::WorkspaceSymbolResponse::Nested(all_symbols))
}
```

- [ ] **Step 3: Rewrite `display.rs`** — same mechanical pattern: remove `async`, remove `server.with_analysis()` wrapper, use `snap.analysis` and `snap.file_path` directly. All handler signatures become `fn handle_X(snap: GlobalStateSnapshot, params: XParams) -> Option<XResult>`.

- [ ] **Step 4: Rewrite `editing.rs`** — same pattern. `handle_execute_command` takes `&GlobalState` since it iterates all hosts. `handle_code_action` takes `GlobalStateSnapshot`.

- [ ] **Step 5: Commit**

```bash
git add crates/lsp/src/handlers/ crates/lsp/src/conversions.rs
git commit -m "migrate all request handlers to sync"
```

---

## Task 7: Create Main Loop

**Files:**
- Create: `crates/lsp/src/main_loop.rs`

The main loop is the heart of the sync architecture. It receives events from three sources (LSP messages, completed tasks, introspection results) and dispatches them.

- [ ] **Step 1: Create `main_loop.rs`**

```rust
use crossbeam_channel::select;
use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::notification::Notification as _;

use crate::dispatch::{NotificationDispatcher, RequestDispatcher};
use crate::global_state::{GlobalState, TaskResponse};
use crate::handlers;

pub fn main_loop(connection: Connection, state: &mut GlobalState) {
    loop {
        select! {
            // LSP messages from the client
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
                        // Channel closed — client disconnected
                        return;
                    }
                }
            }

            // Completed tasks from the thread pool
            recv(state.task_receiver) -> task => {
                if let Ok(task) = task {
                    handle_task(state, task.response);
                }
            }

            // Introspection results from the async thread
            recv(state.introspection_result_receiver) -> result => {
                if let Ok(result) = result {
                    handle_introspection_result(state, result);
                }
            }
        }
    }
}

fn handle_request(state: &mut GlobalState, req: Request) {
    use lsp_types::request::*;

    // For requests that need a snapshot + pool dispatch, extract URI and spawn
    let req_method = req.method.clone();
    let req_id = req.id.clone();

    // Try to extract and dispatch to pool-based handlers first
    // These handlers run on worker threads with a snapshot
    let dispatched = try_dispatch_to_pool(state, &req);
    if dispatched {
        return;
    }

    // Handlers that run on the main thread
    RequestDispatcher::new(req, state)
        .on_mut::<ExecuteCommand>(|state, params| {
            handlers::editing::handle_execute_command(state, params)
        })
        .on::<WorkspaceSymbol>(|state, params| {
            handlers::navigation::handle_workspace_symbol(state, params)
        })
        .finish();
}

/// Try to dispatch a request to the thread pool with a snapshot.
/// Returns true if the request was handled.
fn try_dispatch_to_pool(state: &mut GlobalState, req: &Request) -> bool {
    use lsp_types::request::*;

    // Helper macro to reduce boilerplate for the common pattern:
    // extract params, get URI, take snapshot, spawn handler on pool
    macro_rules! dispatch_pool {
        ($req:expr, $state:expr, $method:ty, $uri_expr:expr, $handler:path) => {{
            let req_clone = $req.clone();
            match req_clone.extract::<<$method as lsp_types::request::Request>::Params>(
                <$method as lsp_types::request::Request>::METHOD,
            ) {
                Ok((id, params)) => {
                    let uri: &lsp_types::Uri = $uri_expr(&params);
                    $state.spawn_with_snapshot(id, uri, move |snap| {
                        $handler(snap, params)
                    });
                    return true;
                }
                Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
                Err(lsp_server::ExtractError::JsonError { method, error }) => {
                    tracing::error!(%method, %error, "invalid request params");
                    return true;
                }
            }
        }};
    }

    dispatch_pool!(req, state, GotoDefinition,
        |p: &GotoDefinitionParams| &p.text_document_position_params.text_document.uri,
        handlers::navigation::handle_goto_definition);

    dispatch_pool!(req, state, HoverRequest,
        |p: &HoverParams| &p.text_document_position_params.text_document.uri,
        handlers::display::handle_hover);

    dispatch_pool!(req, state, Completion,
        |p: &CompletionParams| &p.text_document_position.text_document.uri,
        handlers::editing::handle_completion);

    dispatch_pool!(req, state, References,
        |p: &ReferenceParams| &p.text_document_position.text_document.uri,
        handlers::navigation::handle_references);

    dispatch_pool!(req, state, DocumentSymbolRequest,
        |p: &DocumentSymbolParams| &p.text_document.uri,
        handlers::navigation::handle_document_symbol);

    dispatch_pool!(req, state, SemanticTokensFullRequest,
        |p: &SemanticTokensParams| &p.text_document.uri,
        handlers::display::handle_semantic_tokens_full);

    dispatch_pool!(req, state, SelectionRangeRequest,
        |p: &SelectionRangeParams| &p.text_document.uri,
        handlers::display::handle_selection_range);

    dispatch_pool!(req, state, CodeActionRequest,
        |p: &CodeActionParams| &p.text_document.uri,
        handlers::editing::handle_code_action);

    dispatch_pool!(req, state, CodeLensRequest,
        |p: &CodeLensParams| &p.text_document.uri,
        handlers::display::handle_code_lens);

    dispatch_pool!(req, state, FoldingRangeRequest,
        |p: &FoldingRangeParams| &p.text_document.uri,
        handlers::display::handle_folding_range);

    dispatch_pool!(req, state, InlayHintRequest,
        |p: &InlayHintParams| &p.text_document.uri,
        handlers::display::handle_inlay_hint);

    dispatch_pool!(req, state, SignatureHelpRequest,
        |p: &SignatureHelpParams| &p.text_document_position_params.text_document.uri,
        handlers::editing::handle_signature_help);

    dispatch_pool!(req, state, Rename,
        |p: &RenameParams| &p.text_document_position.text_document.uri,
        handlers::editing::handle_rename);

    dispatch_pool!(req, state, PrepareRenameRequest,
        |p: &TextDocumentPositionParams| &p.text_document.uri,
        handlers::editing::handle_prepare_rename);

    // Custom methods
    let req_clone = req.clone();
    match req_clone.extract::<crate::server::VirtualFileContentParams>(
        "graphql-analyzer/virtualFileContent",
    ) {
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
    match req_clone.extract::<crate::trace_capture::TraceCaptureParams>(
        "graphql-analyzer/traceCapture",
    ) {
        Ok((id, params)) => {
            let result = handlers::custom::handle_trace_capture(state, params);
            state.respond(lsp_server::Response::new_ok(id, result));
            return true;
        }
        Err(lsp_server::ExtractError::MethodMismatch(_)) => {}
        Err(_) => return true,
    }

    // CodeLensResolve (passthrough, runs on main thread)
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
    use lsp_types::notification::*;

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
        TaskResponse::LogMessage(typ, message) => {
            state.send_notification::<lsp_types::notification::LogMessage>(
                lsp_types::LogMessageParams { typ, message },
            );
        }
        TaskResponse::ShowMessage(typ, message) => {
            state.send_notification::<lsp_types::notification::ShowMessage>(
                lsp_types::ShowMessageParams { typ, message },
            );
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
                tracing::info!("Loaded remote schema from {} as {}", result.url, virtual_uri);
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
```

- [ ] **Step 2: Add custom method handlers**

Create `crates/lsp/src/handlers/custom.rs` for the three custom methods (`virtualFileContent`, `ping`, `traceCapture`) as simple sync functions.

- [ ] **Step 3: Register modules**

Add `mod custom;` to `handlers/mod.rs` and `mod main_loop;` to `lib.rs`.

- [ ] **Step 4: Commit**

```bash
git add crates/lsp/src/main_loop.rs crates/lsp/src/handlers/custom.rs crates/lsp/src/handlers/mod.rs crates/lsp/src/lib.rs
git commit -m "add main event loop with request/notification dispatch"
```

---

## Task 8: Wire Up Server Entry Point and Initialize

**Files:**
- Modify: `crates/lsp/src/main.rs`
- Modify: `crates/lsp/src/lib.rs`
- Modify: `crates/lsp/src/server.rs`

- [ ] **Step 1: Rewrite `main.rs`**

```rust
fn print_version() {
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    let git_dirty = option_env!("VERGEN_GIT_DIRTY").unwrap_or("false");
    let dirty_suffix = if git_dirty == "true" { "-dirty" } else { "" };
    println!("graphql-lsp {version} ({git_sha}{dirty_suffix})");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        print_version();
        return;
    }

    graphql_lsp::run_server();
}
```

- [ ] **Step 2: Rewrite `lib.rs` `run_server()`**

```rust
pub fn run_server() {
    let reload_handle = init_tracing();
    install_panic_hook();

    let (connection, io_threads) = lsp_server::Connection::stdio();

    let server_capabilities = build_server_capabilities();
    let initialization_params = connection
        .initialize(serde_json::to_value(server_capabilities).expect("caps serialize"))
        .expect("initialize handshake");

    let mut state = global_state::GlobalState::new(connection.sender.clone());
    state.trace_capture =
        reload_handle.map(trace_capture::TraceCaptureManager::new);

    // Parse init params and store capabilities + workspace folders
    let init_params: lsp_types::InitializeParams =
        serde_json::from_value(initialization_params).expect("valid init params");

    state.client_capabilities = Some(init_params.capabilities);

    if let Some(folders) = init_params.workspace_folders {
        for folder in folders {
            if let Some(path) = folder.uri.to_file_path() {
                state
                    .workspace
                    .init_workspace_folders
                    .insert(folder.uri.to_string(), path.into_owned());
            }
        }
    }

    // Spawn introspection async runtime thread.
    // The thread receives requests and sends results via separate channels.
    // We create the channels in GlobalState::new() and pass the "other end"
    // of each pair to the thread here. The implementation agent should
    // restructure GlobalState::new() to return the receiver/sender halves
    // that the introspection thread needs, or create them in run_server()
    // and pass them to both GlobalState and the thread.
    spawn_introspection_thread(introspection_request_receiver, introspection_result_sender);

    // Run the initialized handler (load workspaces)
    handle_initialized(&mut state);

    // Enter main loop
    main_loop::main_loop(connection, &mut state);

    io_threads.join().expect("io threads");
}
```

The `build_server_capabilities()` function extracts the `ServerCapabilities` construction from the old `initialize()` method. Since `lsp-server`'s `connection.initialize()` handles the capability negotiation, we advertise all capabilities unconditionally (the client will only use what it supports).

- [ ] **Step 3: Implement `handle_initialized` and workspace loading**

Move the workspace loading logic that was in `initialized()` and `load_workspaces_background()` into a function that runs on the main thread but dispatches heavy work to the thread pool. The key difference from the old code is that workspace loading happens synchronously on the main thread (for mutations) with diagnostics dispatched to the pool. Introspection requests are sent to the async thread.

```rust
fn handle_initialized(state: &mut GlobalState) {
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    // ... log version info via state.send_notification ...

    let folders: Vec<(String, PathBuf)> = state
        .workspace
        .init_workspace_folders
        .drain()
        .collect();

    if folders.is_empty() {
        state.send_notification::<StatusNotification>(StatusParams {
            status: "ready".to_owned(),
            message: Some("No workspace folders".to_owned()),
        });
        return;
    }

    state.send_notification::<StatusNotification>(StatusParams {
        status: "loading".to_owned(),
        message: Some(format!("Loading {} workspace(s)...", folders.len())),
    });

    for (uri, path) in folders {
        load_workspace_config(state, &uri, &path);
    }

    state.send_notification::<StatusNotification>(StatusParams {
        status: "ready".to_owned(),
        message: Some("Project loaded".to_owned()),
    });

    // Register file watchers
    register_file_watchers(state);
}
```

- [ ] **Step 4: Implement the introspection thread**

```rust
fn spawn_introspection_thread(
    request_receiver: crossbeam_channel::Receiver<global_state::IntrospectionRequest>,
    result_sender: crossbeam_channel::Sender<global_state::IntrospectionResult>,
) {
    std::thread::Builder::new()
        .name("introspection-runtime".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime for introspection");

            rt.block_on(async {
                while let Ok(req) = request_receiver.recv() {
                    let mut client = graphql_introspect::IntrospectionClient::new();
                    if let Some(headers) = &req.pending.headers {
                        for (name, value) in headers {
                            client = client.with_header(name, value);
                        }
                    }
                    if let Some(timeout) = req.pending.timeout {
                        client = client.with_timeout(
                            std::time::Duration::from_secs(timeout),
                        );
                    }
                    if let Some(retries) = req.pending.retry {
                        client = client.with_retries(retries);
                    }

                    let url = req.pending.url.clone();
                    let result = match client.execute(&url).await {
                        Ok(response) => {
                            Ok(graphql_introspect::introspection_to_sdl(&response))
                        }
                        Err(e) => Err(e.to_string()),
                    };

                    let _ = result_sender.send(global_state::IntrospectionResult {
                        workspace_uri: req.workspace_uri,
                        project_name: req.project_name,
                        url,
                        result,
                    });
                }
            });
        })
        .expect("spawn introspection thread");
}
```

- [ ] **Step 5: Clean up `server.rs`**

Remove:
- `GraphQLLanguageServer` struct and all its methods
- `impl LanguageServer for GraphQLLanguageServer`
- `load_workspaces_background()` free function
- `load_workspace_config_background()` free function
- `load_all_project_files_background()` free function
- `with_analysis()`, `blocking()` helpers
- `describe_join_error()` (no more `JoinError`)

Keep:
- `VirtualFileContentParams`, `StatusNotification`, `StatusParams`, `PingResponse` (still used)
- `validation_errors_to_diagnostics()` (still used during config loading)

- [ ] **Step 6: Build and fix**

Run: `cargo check -p graphql-lsp`

Fix all remaining compile errors — there will be import path adjustments, type mismatches, and missing `use` statements.

- [ ] **Step 7: Run tests**

Run: `cargo test -p graphql-lsp`

The deadlock regression tests were deleted (structurally impossible now). The `apply_content_change` tests and workspace manager tests should pass.

- [ ] **Step 8: Commit**

```bash
git add crates/lsp/src/
git commit -m "wire up sync main loop and server entry point"
```

---

## Task 9: Clean Up and Validate

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/lsp/Cargo.toml`

- [ ] **Step 1: Remove unused workspace dependency**

Remove `tower-lsp-server` from `[workspace.dependencies]` in the root `Cargo.toml`. Check if any other crate uses it:

Run: `grep -r "tower-lsp" crates/*/Cargo.toml`

If only `crates/lsp` used it, remove from workspace deps.

- [ ] **Step 2: Verify tokio is only used for introspection**

Run: `grep -r "tokio" crates/lsp/src/ --include="*.rs"`

All tokio references should be in the introspection thread spawner only (in `lib.rs`). If any remain elsewhere, remove them.

- [ ] **Step 3: Run full test suite**

Run: `cargo test --workspace`
Run: `cargo clippy -p graphql-lsp`
Run: `cargo fmt --check -p graphql-lsp`

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/lsp/Cargo.toml
git commit -m "remove tower-lsp-server dependency"
```

---

## Task 10: Add Cancellation Support

**Files:**
- Modify: `crates/lsp/src/global_state.rs`
- Modify: `crates/lsp/src/main_loop.rs`

This is the separate, reviewable commit for `$/cancelRequest` support.

- [ ] **Step 1: Add in-flight request tracking to `GlobalState`**

```rust
use std::collections::HashSet;
use lsp_server::RequestId;

// Add to GlobalState struct:
pub struct GlobalState {
    // ... existing fields ...
    /// In-flight request IDs. When a cancel notification arrives, the ID is
    /// removed. Before sending a response, we check if the ID is still here —
    /// if not, the result is dropped silently.
    in_flight: HashSet<RequestId>,
}
```

- [ ] **Step 2: Track requests in the main loop**

In `main_loop.rs`, when a request arrives:
```rust
// In handle_request, before dispatching:
state.in_flight.insert(req.id.clone());
```

When sending a response (in `handle_task`):
```rust
TaskResponse::Response(resp) => {
    // Only send if the request hasn't been cancelled
    if state.in_flight.remove(&resp.id) {
        state.respond(resp);
    } else {
        tracing::debug!(id = ?resp.id, "dropping response for cancelled request");
    }
}
```

- [ ] **Step 3: Handle `$/cancelRequest` notification**

Add to the notification dispatcher in `handle_notification`:
```rust
// Before the main dispatcher chain, handle cancel specially:
if not.method == "$/cancelRequest" {
    if let Ok(params) = serde_json::from_value::<lsp_types::CancelParams>(not.params) {
        let id = match params.id {
            lsp_types::NumberOrString::Number(n) => RequestId::from(n),
            lsp_types::NumberOrString::String(s) => RequestId::from(s),
        };
        if state.in_flight.remove(&id) {
            tracing::debug!(?id, "request cancelled by client");
            // Send an error response so the client knows it was cancelled
            state.respond(lsp_server::Response::new_err(
                id,
                lsp_server::ErrorCode::RequestCanceled as i32,
                "request cancelled".to_owned(),
            ));
        }
        return;
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p graphql-lsp`
Run: `cargo clippy -p graphql-lsp`

- [ ] **Step 5: Commit**

```bash
git add crates/lsp/src/global_state.rs crates/lsp/src/main_loop.rs
git commit -m "add $/cancelRequest support for in-flight request cancellation"
```

---

## Task 11: Update crates/CLAUDE.md

**Files:**
- Modify: `crates/CLAUDE.md`

- [ ] **Step 1: Update the architecture documentation**

Remove the "Async Safety: DashMap and Locks" section — DashMap is no longer used in the LSP crate.

Remove the "Snapshot/Host Lock Discipline" section — the deadlock class it documents is structurally impossible in the sync architecture.

Add a new section describing the sync architecture:

```markdown
## LSP Threading Model

The LSP server uses a sync, single-threaded main loop (not async):

- **Main thread**: Owns all mutable state (`GlobalState`, `WorkspaceManager`, `AnalysisHost` instances). All state mutations happen here. No locks needed.
- **Worker thread pool**: Executes read-only Salsa queries via `Analysis` snapshots. Results return via crossbeam channel.
- **Introspection thread**: Dedicated thread with a Tokio runtime for async HTTP introspection calls.

Snapshots are taken on the main thread and moved to workers. Workers never touch `GlobalState` or `AnalysisHost`. The main thread never blocks on workers — it processes their results as events in the next loop iteration.

This eliminates the entire class of async/sync boundary deadlocks that the previous tower-lsp architecture was vulnerable to.
```

- [ ] **Step 2: Commit**

```bash
git add crates/CLAUDE.md
git commit -m "update architecture docs for sync LSP migration"
```
