mod server;

use server::GraphQLLanguageServer;
use tower_lsp_server::{LspService, Server};

#[cfg(feature = "otel")]
fn init_tracing_with_otel() {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    eprintln!("Initializing OpenTelemetry tracing (endpoint: {otlp_endpoint})");

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&otlp_endpoint)
        .build()
        .expect("Failed to create OTLP exporter");

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

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    opentelemetry::global::set_tracer_provider(provider);
}

fn init_tracing_without_otel() {
    // IMPORTANT: LSP uses stdin/stdout for JSON-RPC communication
    // All logs MUST go to stderr to avoid breaking the protocol
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false) // Disable ANSI colors since LSP output doesn't support them
        .with_target(true) // Include module target in logs for better filtering
        .with_thread_ids(true) // Include thread IDs for async debugging
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();
}

#[tokio::main]
async fn main() {
    #[cfg(feature = "otel")]
    {
        if std::env::var("OTEL_TRACES_ENABLED").is_ok() {
            init_tracing_with_otel();
        } else {
            init_tracing_without_otel();
        }
    }

    #[cfg(not(feature = "otel"))]
    init_tracing_without_otel();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(GraphQLLanguageServer::new);

    Server::new(stdin, stdout, socket).serve(service).await;

    #[cfg(feature = "otel")]
    {
        if std::env::var("OTEL_TRACES_ENABLED").is_ok() {
            // OpenTelemetry 0.31+ uses drop-based cleanup, no explicit shutdown needed
        }
    }
}
