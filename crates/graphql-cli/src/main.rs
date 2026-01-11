mod analysis;
mod commands;
mod progress;

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

    /// Schema-related commands (download, etc.)
    Schema {
        #[command(subcommand)]
        command: commands::schema::SchemaCommands,
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
    #[cfg(feature = "otel")]
    let otel_guard = init_telemetry();

    #[cfg(not(feature = "otel"))]
    init_tracing();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Validate { format, watch } => {
            commands::validate::run(cli.config, cli.project.as_deref(), format, watch)
        }
        Commands::Lint { format, watch } => {
            commands::lint::run(cli.config, cli.project.as_deref(), format, watch)
        }
        Commands::Check { base, head } => {
            commands::check::run(cli.config, cli.project, base, head).await
        }
        Commands::Schema { command } => commands::schema::run(command).await,
    };

    #[cfg(feature = "otel")]
    if let Some(provider) = otel_guard {
        eprintln!("Shutting down OpenTelemetry...");
        if let Err(e) = provider.shutdown() {
            eprintln!("Error shutting down tracer provider: {e:?}");
        }
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
fn init_telemetry() -> Option<opentelemetry_sdk::trace::TracerProvider> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::TracerProvider;
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let otel_enabled = std::env::var("OTEL_TRACES_ENABLED")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if !otel_enabled {
        init_tracing();
        return None;
    }

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    eprintln!("Initializing OpenTelemetry with endpoint: {endpoint}");

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()
        .expect("Failed to create OTLP exporter");

    let resource = Resource::new(vec![KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        "graphql-cli",
    )]);

    // Use batch exporter for async, non-blocking trace export
    let provider = TracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    opentelemetry::global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer("graphql-cli");

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    eprintln!("OpenTelemetry initialized. Traces will be sent to {endpoint}");

    Some(provider)
}
