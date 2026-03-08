//! GraphQL Language Server Protocol implementation.
//!
//! This crate provides a GraphQL language server that can be run as a standalone
//! server communicating over stdio. It's typically invoked via `graphql lsp`.

mod conversions;
mod server;
mod workspace;

use server::GraphQLLanguageServer;
use tower_lsp_server::{LspService, Server};

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
/// Returns true if tracing was initialized, false if already initialized.
fn init_tracing_with_otel() -> bool {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Layer;
    use tracing_subscriber::Registry;

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
        return false;
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

    // Per-layer filtering: the fmt layer respects RUST_LOG (defaulting to warn)
    // for quiet stderr output, while the OTEL layer always captures info-level
    // spans so traces flow to the collector regardless of log verbosity.
    let fmt_filter = build_env_filter("warn");
    // The OTEL filter ignores RUST_LOG -- it always captures info-level spans.
    // RUST_LOG controls stderr verbosity, not trace export.
    let otel_filter =
        tracing_subscriber::EnvFilter::new("info,salsa=off");

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_filter(fmt_filter);

    let telemetry_layer = telemetry_layer.with_filter(otel_filter);

    let subscriber = Registry::default()
        .with(fmt_layer)
        .with(telemetry_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return false;
    }

    // The OTLP exporter connects lazily on first span export, so we can't
    // verify connectivity at init time. Connection failures will surface as
    // warnings from the opentelemetry SDK during export.
    eprintln!("OpenTelemetry tracing enabled (endpoint: {otlp_endpoint})");
    eprintln!(
        "Note: the OTLP exporter connects lazily. If no traces appear, \
         verify the collector is running at the configured endpoint."
    );
    opentelemetry::global::set_tracer_provider(provider);
    true
}

/// Initialize basic tracing without OpenTelemetry.
/// Returns true if tracing was initialized, false if already initialized.
fn init_tracing_without_otel() -> bool {
    // IMPORTANT: LSP uses stdin/stdout for JSON-RPC communication
    // All logs MUST go to stderr to avoid breaking the protocol
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false) // Disable ANSI colors since LSP output doesn't support them
        .with_target(true) // Include module target in logs for better filtering
        .with_thread_ids(true) // Include thread IDs for async debugging
        .with_env_filter(build_env_filter("warn"))
        .try_init()
        .is_ok()
}

/// Initialize tracing for the LSP server.
///
/// When `OTEL_TRACES_ENABLED` is set, traces will be sent to an
/// OpenTelemetry collector. Otherwise, logs are written to stderr.
///
/// This function is safe to call even if tracing has already been initialized
/// (e.g., when running as `graphql lsp` subcommand). It will simply skip
/// initialization if a global subscriber is already set.
pub fn init_tracing() {
    if std::env::var("OTEL_TRACES_ENABLED").is_ok() {
        init_tracing_with_otel();
        return;
    }

    init_tracing_without_otel();
}

/// Run the GraphQL language server over stdio.
///
/// This function initializes tracing and starts the LSP server,
/// communicating via stdin/stdout using the JSON-RPC protocol.
///
/// # Example
///
/// ```ignore
/// #[tokio::main]
/// async fn main() {
///     graphql_lsp::run_server().await;
/// }
/// ```
pub async fn run_server() {
    init_tracing();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(GraphQLLanguageServer::new)
        .custom_method(
            "graphql-analyzer/virtualFileContent",
            GraphQLLanguageServer::virtual_file_content,
        )
        .custom_method("graphql-analyzer/ping", GraphQLLanguageServer::ping)
        .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
