//! GraphQL Language Server Protocol implementation.
//!
//! This crate provides a GraphQL language server that can be run as a standalone
//! server communicating over stdio. It's typically invoked via `graphql lsp`.

mod conversions;
mod server;
mod workspace;

use server::GraphQLLanguageServer;
use tower_lsp_server::{LspService, Server};

/// Initialize tracing with OpenTelemetry support (when enabled).
/// Returns true if tracing was initialized, false if already initialized.
#[cfg(feature = "otel")]
fn init_tracing_with_otel() -> bool {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&otlp_endpoint)
        .build()
    {
        Ok(e) => e,
        Err(_) => return false,
    };

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("graphql-lsp");

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(fmt_layer)
        .with(telemetry_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return false;
    }

    eprintln!("Initialized OpenTelemetry tracing (endpoint: {otlp_endpoint})");
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
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .is_ok()
}

/// Initialize tracing for the LSP server.
///
/// When the `otel` feature is enabled and `OTEL_TRACES_ENABLED` is set,
/// traces will be sent to an OpenTelemetry collector. Otherwise, logs
/// are written to stderr.
///
/// This function is safe to call even if tracing has already been initialized
/// (e.g., when running as `graphql lsp` subcommand). It will simply skip
/// initialization if a global subscriber is already set.
pub fn init_tracing() {
    #[cfg(feature = "otel")]
    {
        if std::env::var("OTEL_TRACES_ENABLED").is_ok() {
            init_tracing_with_otel();
            return;
        }
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
            "graphql/virtualFileContent",
            GraphQLLanguageServer::virtual_file_content,
        )
        .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
