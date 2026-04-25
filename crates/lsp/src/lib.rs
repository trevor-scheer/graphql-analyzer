//! GraphQL Language Server Protocol implementation.
//!
//! This crate provides a GraphQL language server that can be run as a standalone
//! server communicating over stdio. It uses a sync main loop with a thread pool
//! for Salsa query execution.

// Under wasm, the native entrypoint (`run_server`) is gated out, so most items
// appear dead until the wasm entrypoint (Task 10) provides a caller.
#![cfg_attr(not(feature = "native"), allow(dead_code, unused_imports))]

mod conversions;
mod dispatch;
mod global_state;
mod handlers;
mod loading;
mod main_loop;
pub(crate) mod server;
pub mod trace_capture;
mod workspace;

use std::path::PathBuf;

use lsp_types::{
    CodeActionKind, CodeActionOptions, CompletionOptions, ExecuteCommandOptions,
    FoldingRangeProviderCapability, HoverProviderCapability, InlayHintOptions,
    InlayHintServerCapabilities, OneOf, RenameOptions, SelectionRangeProviderCapability,
    SemanticTokenModifier, SemanticTokenType, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensServerCapabilities, ServerCapabilities,
    SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncKind,
    WorkDoneProgressOptions,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;

use global_state::GlobalState;
use server::{StatusNotification, StatusParams};

/// Build a tracing `EnvFilter`, always suppressing Salsa's internal logs
/// unless the user explicitly includes `salsa` in `RUST_LOG`.
fn build_env_filter(default: &str) -> tracing_subscriber::EnvFilter {
    let filter_str = std::env::var("RUST_LOG").unwrap_or_else(|_| default.to_string());
    let filter_str = if filter_str.contains("salsa") {
        filter_str
    } else {
        format!("{filter_str},salsa=off")
    };
    tracing_subscriber::EnvFilter::new(filter_str)
}

/// Initialize tracing with OpenTelemetry support.
#[cfg(feature = "native")]
fn init_tracing_with_otel() -> Option<trace_capture::ReloadHandle> {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;

    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let Ok(exporter) = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&otlp_endpoint)
        .build()
    else {
        eprintln!(
            "Failed to build OTLP exporter for endpoint: {otlp_endpoint}. \
             Check that the endpoint URL is valid."
        );
        return None;
    };

    let resource = opentelemetry_sdk::Resource::builder()
        .with_attribute(opentelemetry::KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            "graphql-analyzer",
        ))
        .build();

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("graphql-analyzer");
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let fmt_filter = build_env_filter("warn");
    let otel_filter = tracing_subscriber::EnvFilter::new("info,salsa=off");

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_filter(fmt_filter);

    let telemetry_layer = telemetry_layer.with_filter(otel_filter);

    let (reload_layer, reload_handle) = trace_capture::create_reload_layer();

    let subscriber = Registry::default()
        .with(reload_layer)
        .with(telemetry_layer)
        .with(fmt_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return None;
    }

    eprintln!("OpenTelemetry tracing enabled (endpoint: {otlp_endpoint})");
    eprintln!(
        "Note: the OTLP exporter connects lazily. If no traces appear, \
         verify the collector is running at the configured endpoint."
    );
    opentelemetry::global::set_tracer_provider(provider);
    Some(reload_handle)
}

/// Initialize basic tracing without OpenTelemetry.
fn init_tracing_without_otel() -> Option<trace_capture::ReloadHandle> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_filter(build_env_filter("warn"));

    let (reload_layer, reload_handle) = trace_capture::create_reload_layer();

    let subscriber = Registry::default().with(reload_layer).with(fmt_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return None;
    }

    Some(reload_handle)
}

#[must_use]
pub fn init_tracing() -> Option<trace_capture::ReloadHandle> {
    #[cfg(feature = "native")]
    if std::env::var("OTEL_TRACES_ENABLED").is_ok() {
        return init_tracing_with_otel();
    }
    init_tracing_without_otel()
}

/// Install a panic hook that routes panic info through `tracing`.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = if let Some(l) = info.location() {
            format!("{}:{}:{}", l.file(), l.line(), l.column())
        } else {
            "<unknown>".to_string()
        };
        let payload = info.payload();
        let message = if let Some(s) = payload.downcast_ref::<String>() {
            s.as_str()
        } else if let Some(s) = payload.downcast_ref::<&'static str>() {
            *s
        } else {
            "<non-string panic payload>"
        };
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>");
        let backtrace = std::backtrace::Backtrace::capture();
        tracing::error!(
            thread = thread_name,
            location = %location,
            backtrace = %backtrace,
            "panic: {message}"
        );
        prev(info);
    }));
}

fn build_server_capabilities() -> ServerCapabilities {
    use lsp_types::CodeLensOptions;

    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![
                "{".to_string(),
                "@".to_string(),
                "(".to_string(),
                "$".to_string(),
            ]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(lsp_types::CodeActionProviderCapability::Options(
            CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
                resolve_provider: None,
            },
        )),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: vec![
                        SemanticTokenType::TYPE,
                        SemanticTokenType::PROPERTY,
                        SemanticTokenType::VARIABLE,
                        SemanticTokenType::FUNCTION,
                        SemanticTokenType::ENUM_MEMBER,
                        SemanticTokenType::KEYWORD,
                        SemanticTokenType::STRING,
                        SemanticTokenType::NUMBER,
                    ],
                    token_modifiers: vec![
                        SemanticTokenModifier::DEPRECATED,
                        SemanticTokenModifier::DEFINITION,
                    ],
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            },
        )),
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(true),
        }),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            },
        ))),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec!["graphql-analyzer.checkStatus".to_string()],
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        ..Default::default()
    }
}

#[cfg(feature = "introspect")]
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
                        client = client.with_timeout(std::time::Duration::from_secs(timeout));
                    }
                    if let Some(retries) = req.pending.retry {
                        client = client.with_retries(retries);
                    }

                    let url = req.pending.url.clone();
                    let result = match client.execute(&url).await {
                        Ok(response) => Ok(graphql_introspect::introspection_to_sdl(&response)),
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

#[cfg(feature = "native")]
fn handle_initialized(state: &mut GlobalState) {
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    let git_dirty = option_env!("VERGEN_GIT_DIRTY").unwrap_or("false");
    let build_timestamp = option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("unknown");
    let binary_path =
        std::env::current_exe().map_or_else(|_| "unknown".to_string(), |p| p.display().to_string());

    let dirty_suffix = if git_dirty == "true" { "-dirty" } else { "" };

    tracing::info!(
        version = version,
        git_sha = format!("{git_sha}{dirty_suffix}"),
        build_timestamp = build_timestamp,
        binary_path = binary_path,
        "GraphQL Language Server initialized"
    );

    state.send_notification::<lsp_types::notification::LogMessage>(lsp_types::LogMessageParams {
        typ: lsp_types::MessageType::INFO,
        message: format!(
            "GraphQL LSP initialized (v{version} @ {git_sha}{dirty_suffix}, \
                 built {build_timestamp}, binary: {binary_path})"
        ),
    });

    let folders: Vec<(String, PathBuf)> = state.workspace.init_workspace_folders.drain().collect();

    if folders.is_empty() {
        tracing::debug!("No workspace folders to load");
        state.send_notification::<StatusNotification>(StatusParams {
            status: "ready".to_string(),
            message: Some("No workspace folders".to_string()),
        });
        return;
    }

    state.send_notification::<StatusNotification>(StatusParams {
        status: "loading".to_string(),
        message: Some(format!("Loading {} workspace(s)...", folders.len())),
    });

    let loading_start = std::time::Instant::now();

    for (uri, path) in &folders {
        loading::load_workspace_config(state, uri, path);
    }

    let elapsed = loading_start.elapsed();
    let total_files = state.workspace.file_to_project.len();

    state.send_notification::<StatusNotification>(StatusParams {
        status: "ready".to_string(),
        message: Some(format!(
            "{} files loaded in {:.1}s",
            total_files,
            elapsed.as_secs_f64()
        )),
    });

    register_file_watchers(state);
}

#[cfg(feature = "native")]
fn register_file_watchers(state: &GlobalState) {
    use lsp_types::FileSystemWatcher;

    let config_paths: Vec<PathBuf> = state.workspace.config_paths.values().cloned().collect();

    if config_paths.is_empty() {
        tracing::debug!("No config paths found to watch");
        return;
    }

    let mut watchers: Vec<FileSystemWatcher> = config_paths
        .iter()
        .filter_map(|path| {
            let filename = path.file_name()?.to_str()?;
            Some(FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::String(format!("**/{filename}")),
                kind: Some(lsp_types::WatchKind::all()),
            })
        })
        .collect();

    // Also watch resolved schema files
    for path in state.workspace.resolved_schema_paths.values() {
        if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
            watchers.push(FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::String(format!("**/{filename}")),
                kind: Some(lsp_types::WatchKind::all()),
            });
        }
    }

    let registration = lsp_types::Registration {
        id: "graphql-config-watcher".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(
            serde_json::to_value(lsp_types::DidChangeWatchedFilesRegistrationOptions { watchers })
                .expect("DidChangeWatchedFilesRegistrationOptions is always serializable"),
        ),
    };

    // Send client/registerCapability request. In the sync model we fire this
    // as a request and don't wait for the response (the main loop will handle
    // the Response message when it arrives).
    let params = lsp_types::RegistrationParams {
        registrations: vec![registration],
    };
    let not = lsp_server::Request::new(
        lsp_server::RequestId::from("register-file-watchers".to_string()),
        "client/registerCapability".to_string(),
        params,
    );
    state
        .sender
        .send(lsp_server::Message::Request(not))
        .expect("client channel open");
}

/// Run the GraphQL language server over stdio.
#[cfg(feature = "native")]
pub fn run_server() {
    let reload_handle = init_tracing();
    install_panic_hook();

    let (connection, io_threads) = lsp_server::Connection::stdio();

    let server_capabilities = build_server_capabilities();
    let initialization_params = match connection
        .initialize(serde_json::to_value(server_capabilities).expect("caps serialize"))
    {
        Ok(params) => params,
        Err(e) => {
            // If the protocol-level error is a "request was cancelled" (code -32800),
            // the client disconnected during handshake — exit cleanly.
            if e.channel_is_disconnected() {
                tracing::info!("Client disconnected during initialization");
                return;
            }
            panic!("initialize handshake failed: {e}");
        }
    };

    // Create introspection channels before GlobalState so we can pass them in
    let (introspection_request_sender, introspection_request_receiver) =
        crossbeam_channel::unbounded();
    let (introspection_result_sender, introspection_result_receiver) =
        crossbeam_channel::unbounded();

    let dispatcher: Box<dyn global_state::TaskDispatcher> = Box::new(
        global_state::ThreadPoolDispatcher::new(threadpool::ThreadPool::with_name(
            "salsa-worker".into(),
            num_cpus(),
        )),
    );
    let mut state = global_state::GlobalState::new(
        connection.sender.clone(),
        dispatcher,
        introspection_request_sender,
        introspection_result_receiver,
    );
    state.trace_capture = reload_handle.map(trace_capture::TraceCaptureManager::new);

    let init_params: lsp_types::InitializeParams =
        serde_json::from_value(initialization_params).expect("valid init params");

    state.client_capabilities = Some(init_params.capabilities);

    if let Some(folders) = init_params.workspace_folders {
        for folder in folders {
            if let Some(path) = conversions::uri_to_file_path(&folder.uri) {
                state
                    .workspace
                    .init_workspace_folders
                    .insert(folder.uri.to_string(), path);
            }
        }
    }

    spawn_introspection_thread(introspection_request_receiver, introspection_result_sender);

    handle_initialized(&mut state);

    main_loop::run(&connection, &mut state);

    // Drop the state before joining IO threads to close channels
    drop(state);
    io_threads.join().expect("io threads");
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
}
