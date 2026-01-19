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
    #[arg(short, long, value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    /// Project name (for multi-project configs)
    #[arg(short, long, global = true)]
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

        /// Automatically fix lint issues that have safe fixes
        #[arg(long)]
        fix: bool,
    },

    /// Automatically fix lint issues that have safe fixes
    Fix {
        /// Dry run mode - show what would be fixed without modifying files
        #[arg(long)]
        dry_run: bool,

        /// Only fix specific rule(s), comma-separated
        #[arg(long, value_delimiter = ',')]
        rule: Option<Vec<String>>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,
    },

    /// Run all checks (validate + lint) in a single pass
    ///
    /// This command combines GraphQL spec validation and custom lint rules,
    /// providing unified output and exit codes. Recommended for CI pipelines.
    Check {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,

        /// Watch mode - re-check on file changes
        #[arg(short, long)]
        watch: bool,
    },

    /// List all deprecated field usages across the project
    Deprecations {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,
    },

    /// Schema-related commands (download, etc.)
    Schema {
        #[command(subcommand)]
        command: commands::schema::SchemaCommands,
    },

    /// Display statistics about the GraphQL project
    Stats {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,
    },

    /// Analyze fragment usage across the project
    Fragments {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,
    },

    /// Show schema field coverage by operations
    Coverage {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,

        /// Filter by type name (e.g., "User", "Query")
        #[arg(long, value_name = "TYPE")]
        r#type: Option<String>,
    },

    /// Analyze query complexity for GraphQL operations
    Complexity {
        /// Output format
        #[arg(short, long, value_enum, default_value = "human")]
        format: OutputFormat,

        /// Complexity threshold - exit with error if any operation exceeds this value
        #[arg(short, long)]
        threshold: Option<u32>,

        /// Show per-field complexity breakdown
        #[arg(short, long)]
        breakdown: bool,
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
        Commands::Lint { format, watch, fix } => {
            commands::lint::run(cli.config, cli.project.as_deref(), format, watch, fix)
        }
        Commands::Fix {
            dry_run,
            rule,
            format,
        } => commands::fix::run(cli.config, cli.project.as_deref(), dry_run, rule, format),
        Commands::Check { format, watch } => {
            commands::check::run(cli.config, cli.project.as_deref(), format, watch)
        }
        Commands::Deprecations { format } => {
            commands::deprecations::run(cli.config, cli.project.as_deref(), format)
        }
        Commands::Schema { command } => commands::schema::run(command).await,
        Commands::Stats { format } => {
            commands::stats::run(cli.config, cli.project.as_deref(), format)
        }
        Commands::Fragments { format } => {
            commands::fragments::run(cli.config, cli.project.as_deref(), format)
        }
        Commands::Coverage { format, r#type } => {
            commands::coverage::run(cli.config, cli.project.as_deref(), format, r#type)
        }
        Commands::Complexity {
            format,
            threshold,
            breakdown,
        } => commands::complexity::run(
            cli.config,
            cli.project.as_deref(),
            format,
            threshold,
            breakdown,
        ),
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
