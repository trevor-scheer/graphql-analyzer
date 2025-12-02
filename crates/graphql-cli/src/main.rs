mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "graphql")]
#[command(about = "GraphQL CLI for validation and linting", long_about = None)]
#[command(version)]
struct Cli {
    /// Path to GraphQL config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Project name (for multi-project configs)
    #[arg(short, long)]
    project: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate GraphQL schema and documents against GraphQL spec
    Validate {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,

        /// Watch mode - re-validate on file changes
        #[arg(short, long)]
        watch: bool,
    },

    /// Run custom lint rules on GraphQL documents
    Lint {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,

        /// Watch mode - re-lint on file changes
        #[arg(short, long)]
        watch: bool,
    },

    /// Check for breaking changes between schemas
    Check {
        /// Base branch/ref to compare against
        #[arg(long)]
        base: String,

        /// Head branch/ref to compare
        #[arg(long)]
        head: String,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    /// Human-readable output with colors
    Human,
    /// JSON output for tooling
    Json,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing/logging based on RUST_LOG env var
    #[cfg(feature = "otel")]
    let otel_guard = init_telemetry();

    #[cfg(not(feature = "otel"))]
    init_tracing();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Validate { format, watch } => {
            commands::validate::run(cli.config, cli.project, format, watch).await
        }
        Commands::Lint { format, watch } => {
            commands::lint::run(cli.config, cli.project, format, watch).await
        }
        Commands::Check { base, head } => {
            commands::check::run(cli.config, cli.project, base, head).await
        }
    };

    // Ensure all traces are flushed before exiting
    #[cfg(feature = "otel")]
    if let Some(provider) = otel_guard {
        eprintln!("Shutting down OpenTelemetry...");
        // Explicitly shutdown the provider to flush pending spans
        if let Err(e) = provider.shutdown() {
            eprintln!("Error shutting down tracer provider: {e:?}");
        }
        // Also shutdown the global provider
        opentelemetry::global::shutdown_tracer_provider();
        eprintln!("OpenTelemetry shutdown complete");
    }

    result
}

/// Initialize basic tracing without OpenTelemetry
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .with_writer(std::io::stderr)
        .init();
}

/// Initialize OpenTelemetry tracing with OTLP exporter
#[cfg(feature = "otel")]
#[allow(clippy::too_many_lines)]
#[allow(unused_imports)]
fn init_telemetry() -> Option<opentelemetry_sdk::trace::TracerProvider> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::TracerProvider;
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // Check if OpenTelemetry should be enabled
    let otel_enabled = std::env::var("OTEL_TRACES_ENABLED")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if !otel_enabled {
        // Fall back to regular tracing
        init_tracing();
        return None;
    }

    // Get OTLP endpoint from env or use default
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    eprintln!("Initializing OpenTelemetry with endpoint: {endpoint}");

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()
        .expect("Failed to create OTLP exporter");

    // Create resource with service name
    let resource = Resource::new(vec![KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        "graphql-cli",
    )]);

    // Create tracer provider with resource
    // Use batch exporter for async, non-blocking trace export
    let provider = TracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    // Set as global provider so shutdown_tracer_provider() works
    opentelemetry::global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer("graphql-cli");

    // Create telemetry layer
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    // Create env filter
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // Initialize subscriber with both telemetry and logging
    tracing_subscriber::registry()
        .with(env_filter)
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    eprintln!("OpenTelemetry initialized. Traces will be sent to {endpoint}");

    Some(provider)
}
