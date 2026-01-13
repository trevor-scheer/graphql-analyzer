use crate::analysis::CliAnalysisHost;
use crate::commands::common::CommandContext;
use crate::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

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

    /// Calculate percentage of a given count relative to total operations
    #[allow(clippy::cast_precision_loss)]
    fn percentage(&self, count: usize) -> f64 {
        let total = self.total_operations();
        if total == 0 {
            return 0.0;
        }
        (count as f64 / total as f64) * 100.0
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

    fn min_depth(&self) -> usize {
        self.operation_depths.iter().copied().min().unwrap_or(0)
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

    fn total_fragment_spreads(&self) -> usize {
        self.fragment_spreads_per_operation.iter().sum()
    }
}

/// All project statistics
#[derive(Debug, Default)]
pub(crate) struct ProjectStats {
    pub(crate) schema: graphql_ide::SchemaStats,
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

    let total_ops = stats.documents.total_operations();
    if total_ops > 0 {
        // Show percentages for larger projects (>= 5 operations)
        if total_ops >= 5 {
            println!(
                "  Operations: {} ({} queries {:.0}%, {} mutations {:.0}%, {} subscriptions {:.0}%)",
                total_ops.to_string().bold(),
                stats.documents.queries,
                stats.documents.percentage(stats.documents.queries),
                stats.documents.mutations,
                stats.documents.percentage(stats.documents.mutations),
                stats.documents.subscriptions,
                stats.documents.percentage(stats.documents.subscriptions),
            );
        } else {
            println!(
                "  Operations: {} ({} queries, {} mutations, {} subscriptions)",
                total_ops.to_string().bold(),
                stats.documents.queries,
                stats.documents.mutations,
                stats.documents.subscriptions,
            );
        }
    } else {
        println!("  Operations: {}", "0".bold());
    }

    println!(
        "  Fragments: {}",
        stats.documents.fragments.to_string().bold()
    );

    // Complexity section (only if there are operations)
    if stats.documents.total_operations() > 0 {
        println!();
        println!("{}:", "Complexity".cyan().bold());
        println!(
            "  Operation depth: min {}, avg {:.1}, max {}",
            stats.complexity.min_depth().to_string().bold(),
            stats.complexity.avg_depth(),
            stats.complexity.max_depth().to_string().bold(),
        );
        println!(
            "  Fragment spreads: {} total, {:.1} avg per operation",
            stats.complexity.total_fragment_spreads().to_string().bold(),
            stats.complexity.avg_fragment_spreads(),
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
            "minOperationDepth": stats.complexity.min_depth(),
            "avgOperationDepth": stats.complexity.avg_depth(),
            "maxOperationDepth": stats.complexity.max_depth(),
            "totalFragmentSpreads": stats.complexity.total_fragment_spreads(),
            "avgFragmentSpreadsPerOperation": stats.complexity.avg_fragment_spreads(),
        }
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}

/// Helper implementation for `CliAnalysisHost` to collect stats
impl CliAnalysisHost {
    pub(crate) fn collect_stats(&self) -> ProjectStats {
        let snapshot = self.snapshot();

        // Get schema stats directly from HIR (includes accurate field and directive counts)
        let schema = self.schema_stats();

        // Get document stats from workspace symbols
        let all_symbols = snapshot.workspace_symbols("");
        let mut documents = DocumentStats::default();

        for symbol in &all_symbols {
            match symbol.kind {
                graphql_ide::SymbolKind::Query => documents.queries += 1,
                graphql_ide::SymbolKind::Mutation => documents.mutations += 1,
                graphql_ide::SymbolKind::Subscription => documents.subscriptions += 1,
                graphql_ide::SymbolKind::Fragment => documents.fragments += 1,
                _ => {}
            }
        }

        // Get file count
        documents.files = self.file_count();

        // Collect complexity stats
        let (depths, spreads) = self.complexity_stats();
        let complexity = ComplexityStats {
            operation_depths: depths,
            fragment_spreads_per_operation: spreads,
        };

        ProjectStats {
            schema,
            documents,
            complexity,
        }
    }
}
