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
}

/// Statistics about query complexity
#[derive(Debug, Default)]
pub(crate) struct ComplexityStats {
    pub(crate) operation_depths: Vec<usize>,
    pub(crate) fragment_usages_per_operation: Vec<usize>,
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
    fn avg_fragment_usages(&self) -> f64 {
        if self.fragment_usages_per_operation.is_empty() {
            return 0.0;
        }
        self.fragment_usages_per_operation.iter().sum::<usize>() as f64
            / self.fragment_usages_per_operation.len() as f64
    }

    fn total_fragment_usages(&self) -> usize {
        self.fragment_usages_per_operation.iter().sum()
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
        OutputFormat::Json | OutputFormat::Github => print_json_stats(&stats),
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
    println!("  Types:");
    println!("    Objects: {}", stats.schema.objects.to_string().bold());
    println!(
        "    Inputs: {}",
        stats.schema.input_objects.to_string().bold()
    );
    println!("    Enums: {}", stats.schema.enums.to_string().bold());
    println!(
        "    Interfaces: {}",
        stats.schema.interfaces.to_string().bold()
    );
    println!("    Unions: {}", stats.schema.unions.to_string().bold());
    println!("    Scalars: {}", stats.schema.scalars.to_string().bold());
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
        println!("  Operations:");
        println!(
            "    Queries: {}",
            stats.documents.queries.to_string().bold()
        );
        println!(
            "    Mutations: {}",
            stats.documents.mutations.to_string().bold()
        );
        println!(
            "    Subscriptions: {}",
            stats.documents.subscriptions.to_string().bold()
        );
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
            "  Fragment usages: {} total, {:.1} avg per operation",
            stats.complexity.total_fragment_usages().to_string().bold(),
            stats.complexity.avg_fragment_usages(),
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
            "totalFragmentUsages": stats.complexity.total_fragment_usages(),
            "avgFragmentUsagesPerOperation": stats.complexity.avg_fragment_usages(),
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
        let (depths, usages) = self.complexity_stats();
        let complexity = ComplexityStats {
            operation_depths: depths,
            fragment_usages_per_operation: usages,
        };

        ProjectStats {
            schema,
            documents,
            complexity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_stats_default() {
        let stats = DocumentStats::default();
        assert_eq!(stats.files, 0);
        assert_eq!(stats.queries, 0);
        assert_eq!(stats.mutations, 0);
        assert_eq!(stats.subscriptions, 0);
        assert_eq!(stats.fragments, 0);
    }

    #[test]
    fn test_document_stats_total_operations() {
        let stats = DocumentStats {
            files: 5,
            queries: 10,
            mutations: 5,
            subscriptions: 2,
            fragments: 8,
        };
        assert_eq!(stats.total_operations(), 17);
    }

    #[test]
    fn test_document_stats_total_operations_zero() {
        let stats = DocumentStats::default();
        assert_eq!(stats.total_operations(), 0);
    }

    #[test]
    fn test_complexity_stats_default() {
        let stats = ComplexityStats::default();
        assert!(stats.operation_depths.is_empty());
        assert!(stats.fragment_usages_per_operation.is_empty());
    }

    #[test]
    fn test_complexity_stats_avg_depth() {
        let stats = ComplexityStats {
            operation_depths: vec![2, 4, 6],
            fragment_usages_per_operation: vec![],
        };
        assert!((stats.avg_depth() - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_complexity_stats_avg_depth_empty() {
        let stats = ComplexityStats::default();
        assert!((stats.avg_depth() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_complexity_stats_min_depth() {
        let stats = ComplexityStats {
            operation_depths: vec![5, 2, 8, 3],
            fragment_usages_per_operation: vec![],
        };
        assert_eq!(stats.min_depth(), 2);
    }

    #[test]
    fn test_complexity_stats_min_depth_empty() {
        let stats = ComplexityStats::default();
        assert_eq!(stats.min_depth(), 0);
    }

    #[test]
    fn test_complexity_stats_max_depth() {
        let stats = ComplexityStats {
            operation_depths: vec![5, 2, 8, 3],
            fragment_usages_per_operation: vec![],
        };
        assert_eq!(stats.max_depth(), 8);
    }

    #[test]
    fn test_complexity_stats_max_depth_empty() {
        let stats = ComplexityStats::default();
        assert_eq!(stats.max_depth(), 0);
    }

    #[test]
    fn test_complexity_stats_avg_fragment_usages() {
        let stats = ComplexityStats {
            operation_depths: vec![],
            fragment_usages_per_operation: vec![1, 2, 3, 4],
        };
        assert!((stats.avg_fragment_usages() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_complexity_stats_avg_fragment_usages_empty() {
        let stats = ComplexityStats::default();
        assert!((stats.avg_fragment_usages() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_complexity_stats_total_fragment_usages() {
        let stats = ComplexityStats {
            operation_depths: vec![],
            fragment_usages_per_operation: vec![3, 5, 2],
        };
        assert_eq!(stats.total_fragment_usages(), 10);
    }

    #[test]
    fn test_complexity_stats_total_fragment_usages_empty() {
        let stats = ComplexityStats::default();
        assert_eq!(stats.total_fragment_usages(), 0);
    }

    #[test]
    fn test_project_stats_default() {
        let stats = ProjectStats::default();
        assert_eq!(stats.documents.files, 0);
        assert!(stats.complexity.operation_depths.is_empty());
    }
}
