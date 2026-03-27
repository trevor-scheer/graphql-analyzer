//! Support for inline lint ignore comments.
//!
//! Users can suppress lint diagnostics on a per-case basis using comments:
//!
//! ```graphql
//! # graphql-analyzer-ignore
//! query { ... }
//!
//! # graphql-analyzer-ignore: no_deprecated, unused_variables
//! query { ... }
//! ```
//!
//! The comment must appear on the line immediately before the diagnostic.
//! Without rule names, all lint rules are suppressed for that line.

/// A parsed ignore directive from a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreDirective {
    /// The line number this directive appears on (0-based).
    pub line: usize,
    /// The rules to ignore. Empty means ignore all rules.
    pub rules: Vec<String>,
}

impl IgnoreDirective {
    /// Returns true if this directive suppresses the given rule.
    #[must_use]
    pub fn suppresses(&self, rule_name: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|r| r == rule_name)
    }
}

const IGNORE_PREFIX: &str = "graphql-analyzer-ignore";

/// Parse all ignore directives from GraphQL source text.
///
/// Scans each line for comments matching `# graphql-analyzer-ignore`
/// or `# graphql-analyzer-ignore: rule1, rule2`.
#[must_use]
pub fn parse_ignore_directives(source: &str) -> Vec<IgnoreDirective> {
    let mut directives = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // GraphQL comments start with #
        let Some(comment_body) = trimmed.strip_prefix('#') else {
            continue;
        };

        let comment_body = comment_body.trim();

        let Some(rest) = comment_body.strip_prefix(IGNORE_PREFIX) else {
            continue;
        };

        let rest = rest.trim();

        if rest.is_empty() {
            // Bare ignore: suppress all rules on the next line
            directives.push(IgnoreDirective {
                line: line_num,
                rules: Vec::new(),
            });
        } else if let Some(rule_list) = rest.strip_prefix(':') {
            // Specific rules: `# graphql-analyzer-ignore: rule1, rule2`
            let rules = rule_list
                .split(',')
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect();
            directives.push(IgnoreDirective {
                line: line_num,
                rules,
            });
        }
        // Ignore malformed directives (e.g. `# graphql-analyzer-ignorefoo`)
    }

    directives
}

/// Check if a diagnostic at the given line should be suppressed.
///
/// A diagnostic is suppressed if there is an ignore directive on the
/// immediately preceding line that covers the diagnostic's rule.
#[must_use]
pub fn is_suppressed(directives: &[IgnoreDirective], diagnostic_line: usize, rule: &str) -> bool {
    if diagnostic_line == 0 {
        return false;
    }

    let preceding_line = diagnostic_line - 1;
    directives
        .iter()
        .any(|d| d.line == preceding_line && d.suppresses(rule))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_ignore() {
        let source = "# graphql-analyzer-ignore\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].line, 0);
        assert!(directives[0].rules.is_empty());
        assert!(directives[0].suppresses("any_rule"));
    }

    #[test]
    fn parse_ignore_with_rules() {
        let source = "# graphql-analyzer-ignore: no_deprecated, unused_variables\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(
            directives[0].rules,
            vec!["no_deprecated", "unused_variables"]
        );
        assert!(directives[0].suppresses("no_deprecated"));
        assert!(directives[0].suppresses("unused_variables"));
        assert!(!directives[0].suppresses("other_rule"));
    }

    #[test]
    fn parse_ignore_with_extra_whitespace() {
        let source =
            "  #  graphql-analyzer-ignore :  no_deprecated ,  unused_variables  \nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(
            directives[0].rules,
            vec!["no_deprecated", "unused_variables"]
        );
    }

    #[test]
    fn no_directives_in_regular_comments() {
        let source = "# This is a regular comment\n# Another comment\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert!(directives.is_empty());
    }

    #[test]
    fn malformed_directive_ignored() {
        let source = "# graphql-analyzer-ignorefoo\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert!(directives.is_empty());
    }

    #[test]
    fn multiple_directives() {
        let source = "\
# graphql-analyzer-ignore: no_deprecated
query Foo { hello }
# graphql-analyzer-ignore
query Bar { world }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].line, 0);
        assert_eq!(directives[1].line, 2);
    }

    #[test]
    fn is_suppressed_on_preceding_line() {
        let directives = vec![IgnoreDirective {
            line: 2,
            rules: vec!["no_deprecated".to_string()],
        }];
        assert!(is_suppressed(&directives, 3, "no_deprecated"));
        assert!(!is_suppressed(&directives, 3, "other_rule"));
        assert!(!is_suppressed(&directives, 4, "no_deprecated"));
    }

    #[test]
    fn is_suppressed_bare_ignore() {
        let directives = vec![IgnoreDirective {
            line: 0,
            rules: Vec::new(),
        }];
        assert!(is_suppressed(&directives, 1, "any_rule"));
        assert!(is_suppressed(&directives, 1, "another_rule"));
    }

    #[test]
    fn is_suppressed_line_zero() {
        let directives = vec![IgnoreDirective {
            line: 0,
            rules: Vec::new(),
        }];
        // Can't suppress line 0 since there's no preceding line
        assert!(!is_suppressed(&directives, 0, "any_rule"));
    }

    #[test]
    fn single_rule_ignore() {
        let source = "# graphql-analyzer-ignore: no_anonymous_operations\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rules, vec!["no_anonymous_operations"]);
    }
}
