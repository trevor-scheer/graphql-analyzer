use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

/// Statistics about schema types
#[derive(Debug, Default)]
pub(crate) struct SchemaStats {
    pub(crate) objects: usize,
    pub(crate) interfaces: usize,
    pub(crate) unions: usize,
    pub(crate) enums: usize,
    pub(crate) scalars: usize,
    pub(crate) input_objects: usize,
    pub(crate) total_fields: usize,
    pub(crate) directives: usize,
}

impl SchemaStats {
    fn total_types(&self) -> usize {
        self.objects
            + self.interfaces
            + self.unions
            + self.enums
            + self.scalars
            + self.input_objects
    }
}

/// Statistics about document definitions
#[derive(Debug, Default)]
pub(crate) struct DocumentStats {
    pub(crate) files: usize,
    pub(crate) queries: usize,
    pub(crate) mutations: usize,
    pub(crate) subscriptions: usize,
    pub(crate) fragments: usize,
}

impl DocumentStats {
    fn total_operations(&self) -> usize {
        self.queries + self.mutations + self.subscriptions
    }
}

/// Statistics about query complexity
#[derive(Debug, Default)]
pub(crate) struct ComplexityStats {
    pub(crate) operation_depths: Vec<usize>,
    pub(crate) fragment_spreads_per_operation: Vec<usize>,
}

impl ComplexityStats {
    #[allow(clippy::cast_precision_loss)]
    fn avg_depth(&self) -> f64 {
        if self.operation_depths.is_empty() {
            return 0.0;
        }
        self.operation_depths.iter().sum::<usize>() as f64 / self.operation_depths.len() as f64
    }

    fn max_depth(&self) -> usize {
        self.operation_depths.iter().copied().max().unwrap_or(0)
    }

    #[allow(clippy::cast_precision_loss)]
    fn avg_fragment_spreads(&self) -> f64 {
        if self.fragment_spreads_per_operation.is_empty() {
            return 0.0;
        }
        self.fragment_spreads_per_operation.iter().sum::<usize>() as f64
            / self.fragment_spreads_per_operation.len() as f64
    }
}

/// All project statistics
#[derive(Debug, Default)]
pub(crate) struct ProjectStats {
    pub(crate) schema: SchemaStats,
    pub(crate) documents: DocumentStats,
    pub(crate) complexity: ComplexityStats,
}

pub fn run(
    config_path: Option<PathBuf>,
    project_name: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Load config and validate project requirement
    let ctx = CommandContext::load(config_path, project_name, "stats")?;

    // Get project config
    let selected_name = CommandContext::get_project_name(project_name);
    let project_config = ctx
        .config
        .projects()
        .find(|(name, _)| *name == selected_name)
        .map(|(_, cfg)| cfg.clone())
        .ok_or_else(|| anyhow::anyhow!("Project '{selected_name}' not found"))?;

    // Load project
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Loading schema and documents..."))
    } else {
        None
    };

    let host = CliAnalysisHost::from_project_config(&project_config, &ctx.base_dir)?;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Collect statistics
    let spinner = if matches!(format, OutputFormat::Human) {
        Some(crate::progress::spinner("Collecting statistics..."))
    } else {
        None
    };

    let stats = host.collect_stats();

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Display statistics
    match format {
        OutputFormat::Human => print_human_stats(&stats),
        OutputFormat::Json => print_json_stats(&stats),
    }

    Ok(())
}

fn print_human_stats(stats: &ProjectStats) {
    println!();
    println!("{}", "GraphQL Project Statistics".bold());
    println!("{}", "==========================".dimmed());

    // Schema section
    println!();
    println!("{}:", "Schema".cyan().bold());
    println!(
        "  Types: {} ({} objects, {} inputs, {} enums, {} interfaces, {} unions, {} scalars)",
        stats.schema.total_types().to_string().bold(),
        stats.schema.objects,
        stats.schema.input_objects,
        stats.schema.enums,
        stats.schema.interfaces,
        stats.schema.unions,
        stats.schema.scalars,
    );
    println!("  Fields: {}", stats.schema.total_fields.to_string().bold());
    if stats.schema.directives > 0 {
        println!(
            "  Directives: {}",
            stats.schema.directives.to_string().bold()
        );
    }

    // Documents section
    println!();
    println!("{}:", "Documents".cyan().bold());
    println!("  Files: {}", stats.documents.files.to_string().bold());
    println!(
        "  Operations: {} ({} queries, {} mutations, {} subscriptions)",
        stats.documents.total_operations().to_string().bold(),
        stats.documents.queries,
        stats.documents.mutations,
        stats.documents.subscriptions,
    );
    println!(
        "  Fragments: {}",
        stats.documents.fragments.to_string().bold()
    );

    // Complexity section (only if there are operations)
    if stats.documents.total_operations() > 0 {
        println!();
        println!("{}:", "Complexity".cyan().bold());
        println!(
            "  Avg operation depth: {}",
            format!("{:.1}", stats.complexity.avg_depth()).bold()
        );
        println!(
            "  Max operation depth: {}",
            stats.complexity.max_depth().to_string().bold()
        );
        println!(
            "  Avg fragment spreads per operation: {}",
            format!("{:.1}", stats.complexity.avg_fragment_spreads()).bold()
        );
    }

    println!();
}

fn print_json_stats(stats: &ProjectStats) {
    let json = serde_json::json!({
        "schema": {
            "types": {
                "total": stats.schema.total_types(),
                "objects": stats.schema.objects,
                "interfaces": stats.schema.interfaces,
                "unions": stats.schema.unions,
                "enums": stats.schema.enums,
                "scalars": stats.schema.scalars,
                "inputObjects": stats.schema.input_objects,
            },
            "fields": stats.schema.total_fields,
            "directives": stats.schema.directives,
        },
        "documents": {
            "files": stats.documents.files,
            "operations": {
                "total": stats.documents.total_operations(),
                "queries": stats.documents.queries,
                "mutations": stats.documents.mutations,
                "subscriptions": stats.documents.subscriptions,
            },
            "fragments": stats.documents.fragments,
        },
        "complexity": {
            "avgOperationDepth": stats.complexity.avg_depth(),
            "maxOperationDepth": stats.complexity.max_depth(),
            "avgFragmentSpreadsPerOperation": stats.complexity.avg_fragment_spreads(),
        }
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}

/// Helper implementation for `CliAnalysisHost` to collect stats
impl CliAnalysisHost {
    pub(crate) fn collect_stats(&self) -> ProjectStats {
        // We need to access the internal host and project files to gather stats.
        // For now, let's use the snapshot to get what we can.
        let snapshot = self.snapshot();

        // Get schema types from workspace symbols (types matching any query)
        let all_symbols = snapshot.workspace_symbols("");

        let mut stats = ProjectStats::default();

        // Count types by kind
        for symbol in &all_symbols {
            match symbol.kind {
                graphql_ide::SymbolKind::Type => stats.schema.objects += 1,
                graphql_ide::SymbolKind::Interface => stats.schema.interfaces += 1,
                graphql_ide::SymbolKind::Union => stats.schema.unions += 1,
                graphql_ide::SymbolKind::Enum => stats.schema.enums += 1,
                graphql_ide::SymbolKind::Scalar => stats.schema.scalars += 1,
                graphql_ide::SymbolKind::Input => stats.schema.input_objects += 1,
                graphql_ide::SymbolKind::Query => stats.documents.queries += 1,
                graphql_ide::SymbolKind::Mutation => stats.documents.mutations += 1,
                graphql_ide::SymbolKind::Subscription => stats.documents.subscriptions += 1,
                graphql_ide::SymbolKind::Fragment => stats.documents.fragments += 1,
                _ => {}
            }
        }

        // Get file count and field count from loaded files
        let (file_count, field_count) = self.file_and_field_stats();
        stats.documents.files = file_count;
        stats.schema.total_fields = field_count;

        // Collect complexity stats
        let (depths, spreads) = self.complexity_stats();
        stats.complexity.operation_depths = depths;
        stats.complexity.fragment_spreads_per_operation = spreads;

        stats
    }
}
