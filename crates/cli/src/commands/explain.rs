use colored::Colorize;
use graphql_linter::{all_rule_info, all_rule_names};

pub fn run(rule_name: &str) -> anyhow::Result<()> {
    let rules = all_rule_info();

    let Some(rule) = rules.iter().find(|r| r.name == rule_name) else {
        return Err(not_found_error(rule_name));
    };

    println!("{}", rule.name.bold());
    println!("  {}", rule.description);
    println!();
    println!("  {}: {}", "Severity".dimmed(), rule.default_severity);
    println!("  {}: {}", "Category".dimmed(), rule.category);

    Ok(())
}

fn not_found_error(rule_name: &str) -> anyhow::Error {
    let all_names = all_rule_names();
    let suggestions: Vec<_> = all_names
        .iter()
        .filter(|name| {
            name.to_lowercase().contains(&rule_name.to_lowercase())
                || rule_name.to_lowercase().contains(&name.to_lowercase())
        })
        .copied()
        .collect();

    let hint = if suggestions.is_empty() {
        let names = all_names.join(", ");
        format!("Available rules: {names}")
    } else {
        let names = suggestions.join(", ");
        format!("Did you mean one of: {names}?")
    };

    anyhow::anyhow!("Unknown rule: {rule_name}\n\n{hint}")
}
