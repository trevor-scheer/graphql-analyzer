use colored::Colorize;
use graphql_linter::{all_rule_info, RuleCategory};

#[allow(clippy::unnecessary_wraps)]
pub fn run() -> anyhow::Result<()> {
    let rules = all_rule_info();

    let schema_rules: Vec<_> = rules
        .iter()
        .filter(|r| r.category == RuleCategory::Schema)
        .collect();
    let document_rules: Vec<_> = rules
        .iter()
        .filter(|r| r.category == RuleCategory::Document)
        .collect();
    let project_rules: Vec<_> = rules
        .iter()
        .filter(|r| r.category == RuleCategory::Project)
        .collect();

    // Find the longest rule name for alignment
    let max_name_len = rules.iter().map(|r| r.name.len()).max().unwrap_or(0);

    println!("{}", "Available lint rules:".bold());

    if !schema_rules.is_empty() {
        println!();
        println!("  {}:", "Schema rules".cyan().bold());
        for rule in &schema_rules {
            println!(
                "    {:<width$}  {}",
                rule.name,
                rule.description.dimmed(),
                width = max_name_len
            );
        }
    }

    if !document_rules.is_empty() {
        println!();
        println!("  {}:", "Document rules".cyan().bold());
        for rule in &document_rules {
            println!(
                "    {:<width$}  {}",
                rule.name,
                rule.description.dimmed(),
                width = max_name_len
            );
        }
    }

    if !project_rules.is_empty() {
        println!();
        println!("  {}:", "Project rules".cyan().bold());
        for rule in &project_rules {
            println!(
                "    {:<width$}  {}",
                rule.name,
                rule.description.dimmed(),
                width = max_name_len
            );
        }
    }

    println!();
    println!(
        "  {} rules available. Use {} for details.",
        rules.len().to_string().bold(),
        "graphql explain <rule>".green()
    );

    Ok(())
}
