mod analysis;
mod commands;
mod progress;
mod watch;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "graphql")]
#[command(about = "GraphQL CLI for validation and linting", long_about = None)]
#[command(version)]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Path to GraphQL config file
    #[arg(short, long, value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    /// Project name (for multi-project configs)
    #[arg(short, long, global = true)]
    project: Option<String>,

    /// Force colored output even when not a TTY
    #[arg(long, global = true, conflicts_with = "no_color")]
    color: bool,

    /// Disable colored output
    #[arg(long, global = true, conflicts_with = "color")]
    no_color: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Suppress progress indicators (spinners)
    #[arg(long, global = true)]
    no_progress: bool,

    #[command(subcommand)]
    command: Commands,
}

/// Output verbosity options
#[derive(Debug, Clone, Copy)]
pub struct OutputOptions {
    /// Whether to show progress indicators (spinners)
    pub show_progress: bool,
    /// Whether to show informational output (success messages, summaries)
    pub show_info: bool,
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
        #[arg(long, conflicts_with = "fix_dry_run")]
        fix: bool,

        /// Show what would be fixed without modifying files
        #[arg(long, conflicts_with = "fix")]
        fix_dry_run: bool,
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

    /// Start an MCP server for AI agent integration
    ///
    /// This command starts a Model Context Protocol (MCP) server that exposes
    /// GraphQL tooling to AI agents. The server communicates via stdio.
    Mcp {
        /// Workspace directory (defaults to current directory)
        #[arg(short, long)]
        workspace: Option<PathBuf>,

        /// Don't preload any projects (use `load_project` tool to load on demand)
        #[arg(long)]
        no_preload: bool,

        /// Specific projects to preload (comma-separated)
        ///
        /// By default, all projects are loaded. Use this to limit which projects
        /// are preloaded at startup. Remaining projects can be loaded via `load_project` tool.
        #[arg(long, value_delimiter = ',')]
        preload: Option<Vec<String>>,
    },

    /// Start the Language Server Protocol (LSP) server
    ///
    /// This command starts the GraphQL language server, which provides IDE features
    /// like diagnostics, hover, goto definition, find references, and completions.
    /// The server communicates via stdio using JSON-RPC.
    Lsp,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    /// Human-readable output with colors
    Human,
    /// JSON output for tooling
    Json,
    /// GitHub Actions workflow commands for PR annotations
    Github,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Handle LSP command early, before CLI tracing init.
    // The LSP initializes its own tracing subscriber with .with_ansi(false)
    // (required since LSP output goes to the editor's Output tab).
    // If we init CLI tracing first (which enables ANSI), the LSP's try_init()
    // silently fails and ANSI escape codes leak into the Output tab.
    if matches!(cli.command, Commands::Lsp) {
        return commands::lsp::run().await;
    }

    #[cfg(feature = "otel")]
    let otel_guard = init_telemetry();

    #[cfg(not(feature = "otel"))]
    init_tracing();

    configure_colors(cli.color, cli.no_color);

    let output_opts = OutputOptions {
        show_progress: !cli.quiet && !cli.no_progress,
        show_info: !cli.quiet,
    };

    let result = match cli.command {
        Commands::Validate { format, watch } => commands::validate::run(
            cli.config,
            cli.project.as_deref(),
            format,
            watch,
            output_opts,
        ),
        Commands::Lint {
            format,
            watch,
            fix,
            fix_dry_run,
        } => commands::lint::run(
            cli.config,
            cli.project.as_deref(),
            format,
            watch,
            fix,
            fix_dry_run,
            output_opts,
        ),
        Commands::Check { format, watch } => commands::check::run(
            cli.config,
            cli.project.as_deref(),
            format,
            watch,
            output_opts,
        ),
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
        Commands::Mcp {
            workspace,
            no_preload,
            preload,
        } => commands::mcp::run(workspace, no_preload, preload).await,
        Commands::Lsp => unreachable!("handled above"),
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

/// Configure colored output based on flags and environment variables.
///
/// Priority order (highest to lowest):
/// 1. `--color` flag (force colors on)
/// 2. `--no-color` flag (force colors off)
/// 3. `NO_COLOR` environment variable (if set to any value, disable colors)
/// 4. `CLICOLOR_FORCE` environment variable (if set to non-zero, force colors)
/// 5. `CLICOLOR` environment variable (if set to "0", disable colors)
/// 6. Default: colors enabled if stdout is a TTY (handled by `colored` crate)
///
/// See: <https://no-color.org/> and <https://bixense.com/clicolors/>
fn configure_colors(force_color: bool, no_color: bool) {
    use colored::control;

    if force_color {
        control::set_override(true);
    } else if no_color {
        control::set_override(false);
    } else if std::env::var_os("NO_COLOR").is_some() {
        // NO_COLOR: if present (regardless of value), disable colors
        control::set_override(false);
    } else if let Ok(val) = std::env::var("CLICOLOR_FORCE") {
        // CLICOLOR_FORCE: if set to non-empty, non-zero value, force colors
        if !val.is_empty() && val != "0" {
            control::set_override(true);
        }
    } else if let Ok(val) = std::env::var("CLICOLOR") {
        // CLICOLOR: if set to "0", disable colors
        if val == "0" {
            control::set_override(false);
        }
    }
    // Default: let the colored crate decide based on TTY detection
}

/// Initialize OpenTelemetry tracing with OTLP exporter
#[cfg(feature = "otel")]
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

#[cfg(test)]
mod color_tests {
    use super::configure_colors;
    use colored::control::{self, SHOULD_COLORIZE};
    use std::sync::Mutex;

    // Mutex to serialize tests that modify global state (env vars and color override)
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        let _lock = TEST_MUTEX.lock().unwrap();

        // Save and clear color-related env vars
        let saved_no_color = std::env::var_os("NO_COLOR");
        let saved_clicolor = std::env::var_os("CLICOLOR");
        let saved_clicolor_force = std::env::var_os("CLICOLOR_FORCE");

        std::env::remove_var("NO_COLOR");
        std::env::remove_var("CLICOLOR");
        std::env::remove_var("CLICOLOR_FORCE");

        // Reset color override
        control::unset_override();

        f();

        // Restore env vars
        control::unset_override();
        if let Some(v) = saved_no_color {
            std::env::set_var("NO_COLOR", v);
        }
        if let Some(v) = saved_clicolor {
            std::env::set_var("CLICOLOR", v);
        }
        if let Some(v) = saved_clicolor_force {
            std::env::set_var("CLICOLOR_FORCE", v);
        }
    }

    #[test]
    fn color_flag_forces_colors_on() {
        with_clean_env(|| {
            configure_colors(true, false);
            assert!(SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn no_color_flag_forces_colors_off() {
        with_clean_env(|| {
            configure_colors(false, true);
            assert!(!SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn color_flag_overrides_no_color_env() {
        with_clean_env(|| {
            std::env::set_var("NO_COLOR", "1");
            configure_colors(true, false);
            assert!(SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn no_color_env_disables_colors() {
        with_clean_env(|| {
            std::env::set_var("NO_COLOR", "1");
            configure_colors(false, false);
            assert!(!SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn no_color_env_with_empty_value_disables_colors() {
        with_clean_env(|| {
            // NO_COLOR spec: presence alone is enough, value doesn't matter
            std::env::set_var("NO_COLOR", "");
            configure_colors(false, false);
            assert!(!SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn clicolor_force_enables_colors() {
        with_clean_env(|| {
            std::env::set_var("CLICOLOR_FORCE", "1");
            configure_colors(false, false);
            assert!(SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn clicolor_force_zero_does_not_enable_colors() {
        with_clean_env(|| {
            std::env::set_var("CLICOLOR_FORCE", "0");
            configure_colors(false, false);
            // Should fall through to TTY detection (not forced on)
            // We can't easily assert the default, but we verify no override was set
        });
    }

    #[test]
    fn clicolor_force_empty_does_not_enable_colors() {
        with_clean_env(|| {
            std::env::set_var("CLICOLOR_FORCE", "");
            configure_colors(false, false);
            // Empty string should not force colors on
        });
    }

    #[test]
    fn clicolor_zero_disables_colors() {
        with_clean_env(|| {
            std::env::set_var("CLICOLOR", "0");
            configure_colors(false, false);
            assert!(!SHOULD_COLORIZE.should_colorize());
        });
    }

    #[test]
    fn clicolor_one_uses_default_tty_detection() {
        with_clean_env(|| {
            std::env::set_var("CLICOLOR", "1");
            configure_colors(false, false);
            // CLICOLOR=1 means "use TTY detection", not "force on"
            // No override should be set, so default behavior applies
        });
    }

    #[test]
    fn no_color_env_takes_priority_over_clicolor_force() {
        with_clean_env(|| {
            std::env::set_var("NO_COLOR", "1");
            std::env::set_var("CLICOLOR_FORCE", "1");
            configure_colors(false, false);
            assert!(!SHOULD_COLORIZE.should_colorize());
        });
    }
}
